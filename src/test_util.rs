//! Helper module for making tests.
use std::{env, fs};
use std::path::PathBuf;

use rand::distributions::Alphanumeric;
use rand::Rng;

/// Creates a directory in the given path and deletes it on Drop
#[derive(Debug)]
pub struct TempDirectoryHandler {
    path: PathBuf,
}

impl TempDirectoryHandler {
    /// Builds a handler for temporary directories
    pub fn build() -> Option<Self> {
        // Tests that the manager is initializing the directories as required
        let test_dir: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(7)
            .map(char::from)
            .collect();

        let mut path = env::temp_dir();
        path.push(test_dir);

        if fs::create_dir(&path).is_ok() {
            Some(TempDirectoryHandler { path })
        } else {
            None
        }
    }
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempDirectoryHandler {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use super::*;

    #[test]
    fn directory_handler() {
        // Create default directory
        let temp_dir = TempDirectoryHandler::build().unwrap();
        assert!(temp_dir.path().exists());

        // Create a file within the directory
        let mut file_path = temp_dir.path().clone();
        file_path.push("test_file.txt");

        // Try creating a file within
        let contents = "Some contents";
        fs::write(&file_path, contents).unwrap();
        assert!(file_path.exists());

        // Try removing all
        let path = temp_dir.path().clone();
        drop(temp_dir);
        assert!(!path.exists());
    }
}