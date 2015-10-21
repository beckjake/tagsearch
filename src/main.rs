extern crate id3;
use id3::Tag;
use std::string;
use std::env;
use std::error;
use std::fs::{self,DirEntry,File};
use std::io::{self,Read};
use std::path::{Path,PathBuf};

#[derive(Debug)]
enum Mp3Error {
    IoError(io::Error),
    Mp3StringError(string::FromUtf8Error),
    InvalidMp3Error(String),
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
            Err(e) => return Ok(false),
        };
        // let result = try!(String::from_utf8(Vec::from(&buffer[0..3])));
        Ok(result == "ID3")
    } else {
        Ok(false)
    }
}


fn walk_dir(root: PathBuf) -> Result<Vec<PathBuf>, Mp3Error> {
    // recursively walk the directory, looking for all paths
    // that look like mp3s and shoving them into a Vec.
    // this is only really necessary because walk_dir is unstable
    // and compiling rust sucks
    let mut vec: Vec<PathBuf> = Vec::new();
    if try!(fs::metadata(&root)).is_dir() {
        for entry in try!(fs::read_dir(root)) {
            // if it's a directory, recurse.
            // let entry = try!(entry);
            let path = try!(entry).path();
            let metadata = try!(fs::metadata(&path));
            if metadata.is_dir() {
                vec.extend(try!(walk_dir(path)));
            } else if metadata.is_file() {
                if try!(is_mp3(&path)) {
                    vec.push(path);
                }
            // } else { //metadata.is_symlink(), I guess?
                // TODO: how should I handle symlinks?
            }
        }
    }
    Ok(vec)
}


fn main() {
    // let mut tag = Tag::read_from_path("sample.mp3").unwrap();
    let args: Vec<String> = env::args().skip(1).collect();
    for arg in args {
        let mp3s: Vec<PathBuf> = match walk_dir(PathBuf::from(&arg)) {
            Ok(paths) => paths,
            Err(e) => panic!("File {} failed: {:?}", arg, e),
        };
        for mp3 in mp3s {
            let mut tag = Tag::read_from_path(mp3).unwrap();
            println!("{} - {} - {}",
                tag.artist().unwrap_or("???"),
                tag.album().unwrap_or("???"),
                tag.title().unwrap_or("???")
            );

        }
    }
}
