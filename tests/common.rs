use std::path::{Path, PathBuf};

use std::io;
use std::io::Write;

pub fn setup_create_file(filename: &str, content: &str) -> io::Result<PathBuf> {
    let mut path = PathBuf::from("tests/resources");
    path.push(filename);

    std::fs::write(&path, content)?;

    Ok(path)
}

pub struct TeardownFile<'a> {
    paths: Vec<&'a Path>,
}

impl<'a> TeardownFile<'a> {
    pub fn new(path: &'a Path) -> TeardownFile {
        TeardownFile { paths: vec![path] }
    }

    pub fn new_vec(paths: Vec<&'a Path>) -> TeardownFile {
        TeardownFile { paths }
    }
}

impl<'a> Drop for TeardownFile<'a> {
    fn drop(&mut self) {
        self.paths
            .iter()
            .filter(|path| path.exists())
            .for_each(|path| {
                let _ = std::fs::remove_file(path);
            });
    }
}
