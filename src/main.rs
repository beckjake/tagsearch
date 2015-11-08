extern crate id3;
use id3::Tag;
use std::string;
use std::env;
use std::fs::{self,File};
use std::io::{self,Read};
use std::path::PathBuf;
use std::fmt;

extern crate rusqlite;
pub use self::rusqlite::SqliteConnection;
use self::rusqlite::SqliteResult;
use self::rusqlite::SqliteError;
use self::rusqlite::types::ToSql;


// Error stuff
#[derive(Debug)]
enum Mp3Error {
    IoError(io::Error),
    Mp3StringError(string::FromUtf8Error),
    Id3Error(id3::Error),
    DatabaseError(SqliteError),
}

impl From<io::Error> for Mp3Error {
    fn from(e: io::Error) -> Mp3Error {
        Mp3Error::IoError(e)
    }
}

impl From<string::FromUtf8Error> for Mp3Error {
    fn from(e: string::FromUtf8Error) -> Mp3Error {
        Mp3Error::Mp3StringError(e)
    }
}

impl From<id3::Error> for Mp3Error {
    fn from(e: id3::Error) -> Mp3Error {
        Mp3Error::Id3Error(e)
    }
}

impl From<SqliteError> for Mp3Error {
    fn from(e: SqliteError) -> Mp3Error {
        Mp3Error::DatabaseError(e)
    }
}


// Searching for files stuff
fn is_mp3(file: &PathBuf) -> Result<bool, Mp3Error> {
    if try!(fs::metadata(&file)).is_file() {
        // this is real lame, but I'm only interested in files that have ID3
        // tags anyway, so...
        let mut f = try!(File::open(file));
        let mut buffer = [0; 3];
        let size = try!(f.read(&mut buffer));
        if size != 3 {
            return Ok(false)
        }
        let result = match String::from_utf8(Vec::from(&buffer[0..3])) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        Ok(result == "ID3")
    } else {
        Ok(false)
    }
}


struct DirectoryWalker {
    directories: Vec<PathBuf>,
    files: Vec<PathBuf>,
    errors: Vec<Mp3Error>
}

impl DirectoryWalker {
    fn new(root: PathBuf) -> DirectoryWalker {
        let mut directories: Vec<PathBuf> = Vec::new();
        directories.push(root);
        DirectoryWalker {
            directories: directories,
            files: Vec::new(),
            errors: Vec::new(),
        }
    }
    fn handle_entry(&mut self, path: PathBuf) -> Option<PathBuf> {

        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                self.errors.push(Mp3Error::from(e));
                return None
            },
        };
        if metadata.is_dir() {
            self.directories.push(path);
            None
        } else if metadata.is_file() && is_mp3(&path).unwrap_or(false) {
            Some(path)
        } else {
            None
        }
    }
    fn repopulate_files(&mut self, dirpath: PathBuf) {
        let readdir = match fs::read_dir(dirpath) {
            Err(e) => {
                self.errors.push(Mp3Error::from(e));
                return
            },
            Ok(r) => r
        };
        for entry in readdir {
            match entry {
                Err(e) => self.errors.push(Mp3Error::from(e)),
                Ok(f) => self.files.push(f.path()),
            }
        };
    }
}

impl Iterator for DirectoryWalker {
    type Item = PathBuf;
    fn next(&mut self) -> Option<PathBuf> {
        loop {
            while !self.files.is_empty() {
                let path = match self.files.pop() {
                    Some(path) => path,
                    None => continue,
                };
                // pushes new things into directories/errors, only returns
                // something when it finds something (or None to try the next file)
                match self.handle_entry(path) {
                    Some(path) => return Some(path),
                    None => continue,
                }
            }
            // done with the current file list. Let's repopulate it.
            let dirpath = match self.directories.pop() {
                Some(dirpath) => dirpath,
                None => return None,
            };
            self.repopulate_files(dirpath);
        }
    }
}






// Database/Tag stuff starts here:


pub struct DBTag {
    track_id: i64,
    path: PathBuf,
    tag: Tag,
}


impl fmt::Debug for DBTag {
    fn fmt(&self, out: &mut fmt::Formatter) -> fmt::Result {
        write!(out, "DBTag(track_id={}, path={:?}, artist={}, album={}, title={})",
            self.track_id, self.path,
            self.tag.artist().unwrap_or("???"),
            self.tag.album().unwrap_or("???"),
            self.tag.title().unwrap_or("???"),
        )
    }
}


fn opt_u32_to_i32(track: Option<u32>) -> Option<i32> {
    match track {
        Some(s) => Some(s as i32),
        None => None,
    }
}

fn clean_tag(tag: Option<&str>) -> Option<String> {
    match tag {
        Some(s) => Some(s.to_string()),
        None => return None,
    }
}



impl DBTag {
    pub fn create(db: &SqliteConnection) -> SqliteResult<()> {
        try!(db.execute_batch("BEGIN; CREATE TABLE tags (
            track_id INTEGER PRIMARY KEY, path TEXT, title TEXT,
            number INTEGER, artist TEXT, album TEXT, genre TEXT
            ); COMMIT;"
        ));
        Ok(())
    }
    pub fn insert(db: &SqliteConnection, tag: Tag, path: &PathBuf) -> SqliteResult<DBTag> {
        let stmt = "INSERT INTO tags (path, title, number, artist, album, genre) VALUES (?, ?, ?, ?, ?, ?)";
        let values: &[&ToSql] = &[&path.to_str(), &clean_tag(tag.title()), &opt_u32_to_i32(tag.track()),
            &clean_tag(tag.artist()), &clean_tag(tag.album()), &clean_tag(tag.genre())
        ];
        let result = db.execute(stmt, values);
        match result {
            Ok(_) => (),
            Err(e) => {
                panic!("error: {:?}", e);
            }
        };
        let row_id = db.last_insert_rowid();
        Ok(DBTag {
            track_id: row_id,
            path: path.clone(),
            tag: Tag::read_from_path(path.clone()).unwrap(),
        })
    }
}



fn store_tag(db: &SqliteConnection, mp3: &PathBuf) -> Result<DBTag, Mp3Error> {
    let tag = try!(Tag::read_from_path(mp3));
    Ok(try!(DBTag::insert(&db, tag, mp3)))
}


fn populate_db_from_dir(db: &SqliteConnection, mp3s: &mut DirectoryWalker) -> Result<Vec<DBTag>, Mp3Error> {
    // can't just loop over it with for, because the borrow checker gets all
    // upset about me using it later to access the errors.
    let mut found: Vec<DBTag> = Vec::new();
    loop {
        match mp3s.next() {
            Some(mp3) => {
                let tag = try!(Tag::read_from_path(&mp3));
                found.push(try!(DBTag::insert(&db, tag, &mp3)));
                ()
            },
            None => break,
        };
    }

    Ok(found)
}




fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let db = SqliteConnection::open("test.db").unwrap();
    DBTag::create(&db).unwrap();
    for arg in args {
        let mut mp3s = DirectoryWalker::new(PathBuf::from(&arg));
        populate_db_from_dir(&db, &mut mp3s).unwrap();
    }
}
