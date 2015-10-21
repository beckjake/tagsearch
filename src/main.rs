extern crate id3;
use id3::Tag;
use std::string;
use std::env;
use std::fs::{self,File};
use std::io::{self,Read};
use std::path::PathBuf;

#[derive(Debug)]
enum Mp3Error {
    IoError(io::Error),
    Mp3StringError(string::FromUtf8Error),
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


fn print_tag(mp3: PathBuf) {
    let tag = Tag::read_from_path(mp3).unwrap();
    println!("{} - {} - {}",
        tag.artist().unwrap_or("???"),
        tag.album().unwrap_or("???"),
        tag.title().unwrap_or("???")
    );
}


fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    for arg in args {
        let mut mp3s = DirectoryWalker::new(PathBuf::from(&arg));
        // can't just loop over it with for, because the borrow checker gets all
        // upset about me using it later to access the errors.
        loop {
            match mp3s.next() {
                Some(mp3) => print_tag(mp3),
                None => break,
            }
        }
        for error in mp3s.errors {
            println!("error: {:?}", error);
        }
    }
}
