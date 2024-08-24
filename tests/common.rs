use std::path::{Path, PathBuf};

use std::io;

/// Creates a new file with the given filename and content, and returns a `TeardownFile`
/// that will automatically delete the file when dropped.
///
/// # Panics
///
/// This function will panic if:
/// - The file cannot be created
/// - The content cannot be written to the file
/// - There are any I/O errors during file creation or writing
pub fn setup_file(filename: &str, content: &str) -> TeardownFile {
    let path_buf = new_file(filename, content).unwrap_or_else(|e| panic!(
        "Failed to create test file. filename: {}, content: {}. Error: {}", filename, content, e));
    TeardownFile::new(path_buf)
}

fn new_file(filename: &str, content: &str) -> io::Result<PathBuf> {
    let mut path = PathBuf::from("tests/temp");
    path.push(filename);

    std::fs::write(&path, content)?;

    Ok(path)
}

pub struct TeardownFile {
    path_buf: PathBuf,
}

impl TeardownFile {
    pub fn new(path_buf: PathBuf) -> TeardownFile {
        TeardownFile { path_buf }
    }

    pub fn path(&self) -> &Path {
        self.path_buf.as_path()
    }
}

impl Drop for TeardownFile {
    fn drop(&mut self) {
        if self.path_buf.exists() {
            let _ = std::fs::remove_file(self.path_buf.clone());
        }
    }
}
