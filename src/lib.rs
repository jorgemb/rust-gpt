use std::path::PathBuf;

use regex::Regex;
use thiserror::Error;
use tokio::{fs, io};

#[cfg(test)]
mod test_util;

#[cfg(test)]
mod tests;

#[derive(Error, Debug)]
pub enum RustGPTError {
    #[error("Couldn't create initial directory: {0}")]
    Initialize(String),

    #[error("Error while working with directory")]
    DirectoryIO(#[from] io::Error),
}

/// Helps with creating, saving and loading conversations.
pub struct ConversationManager {
    base_path: PathBuf,
    conversations: Vec<String>,
}

impl ConversationManager {
    /// The ConversationManager helps managing the directory where all the conversations are stored.
    /// Helps discovering, managing and updating each of the files.
    ///
    /// # Arguments
    ///
    /// * `path`: Base path were all the information should be stored.
    ///
    /// returns: Result<ConversationManager, RustGPTError>
    ///
    pub async fn build<P>(path: P) -> Result<ConversationManager, RustGPTError>
        where P: Into<PathBuf>
    {
        let base_path: PathBuf = path.into();

        // Check if it exists and can be created
        if base_path.exists() && !base_path.is_dir() {
            return Err(RustGPTError::Initialize(format!(
                "Path {} points to a file",
                base_path.display()
            )));
        }
        fs::create_dir_all(&base_path).await?;
        let conversations = Vec::new();

        // Create manager and refresh conversations
        let mut conversation_manager = ConversationManager {
            base_path,
            conversations,
        };
        conversation_manager.refresh_conversations().await?;

        Ok(conversation_manager)
    }

    /// Refreshes the conversations by loading the list of files from the filesystem.
    pub async fn refresh_conversations(&mut self) -> Result<(), RustGPTError> {
        // Create new list
        let mut conversations = Vec::with_capacity(self.conversations.len());

        // Load current conversations
        let conversation_pattern = Regex::new(r"conversation_([0-9]+)\.yaml").expect("Bad regex");
        let mut entries = fs::read_dir(&self.base_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            // Ignore directories
            if path.is_dir() {
                continue;
            }

            // Check if name follows pattern
            if let Some(file_name) = path.file_name() {
                if let Some(file_name) = file_name.to_str() {
                    if conversation_pattern.is_match(file_name) {
                        // This is a conversation
                        conversations.push(file_name.to_string());
                    }
                }
            }
        }

        // Overwrite
        self.conversations = conversations;

        Ok(())
    }
}
