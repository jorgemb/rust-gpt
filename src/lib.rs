use std::collections::HashMap;
use std::path::PathBuf;

use async_openai::types::Role;
use derive_builder::Builder;
use regex::Regex;
use serde::{Deserialize, Serialize};
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

    #[error("Couldn't serialize conversation {0}")]
    SerializeConversation(String),

    #[error("Couldn't write conversation {0}")]
    WriteConversation(String),
}


#[derive(Serialize, Deserialize, Builder)]
pub struct ConversationParameters {
    #[builder(default = "1.0")]
    temperature: f32,

    #[builder(default = "1")]
    n: u8,

    #[builder(default = "String::from(\"gpt-3.5-turbo\")")]
    model: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: Role,
    pub content: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Conversation {
    parameters: ConversationParameters,
    interactions: Vec<ConversationMessage>,

    // Set to true when the conversation has been changed and needs to be saved to disk
    updated: bool,
}

impl Conversation {
    /// Creates a new Conversation object with the provided parameters
    ///
    /// # Arguments
    ///
    /// * `parameters`:
    ///
    /// returns: Conversation
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_gpt::{Conversation, ConversationParametersBuilder};
    /// let parameters = ConversationParametersBuilder::default()
    ///     .n(1)
    ///     .model(String::from("gpt-3.5-turbo"))
    ///     .build()
    ///     .unwrap();
    ///
    /// let conversation = Conversation::new(parameters);
    /// ```
    pub fn new(parameters: ConversationParameters) -> Self {
        Conversation { parameters, interactions: Vec::new(), updated: true }
    }
}

/// Helps with creating, saving and loading conversations.
pub struct ConversationManager {
    base_path: PathBuf,
    conversations: HashMap<String, Option<Conversation>>,
}

impl ConversationManager {
    const CONVERSATION_PREFIX: &'static str = "conversation_";

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
        let conversations = HashMap::new();

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
        let mut conversations = HashMap::with_capacity(self.conversations.len());

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
            let Some(file_name) = path.file_name() else { continue; };
            let Some(file_name) = file_name.to_str() else { continue; };
            let Some(captures) = conversation_pattern.captures(file_name) else { continue; };
            let Some(timestamp) = captures.get(1) else { continue; };
            let timestamp = timestamp.as_str();

            let previous = self.conversations.entry(timestamp.to_string()).or_default().take();
            conversations.insert(timestamp.to_string(), previous);
        }

        // Overwrite
        self.conversations = conversations;

        Ok(())
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }
    pub fn conversations(&self) -> Vec<String> {
        self.conversations.keys().cloned().collect()
    }

    /// Returns the conversation with the given name
    pub fn get_conversation(&mut self, name: &str) -> Option<&mut Conversation> {
        self.conversations.entry(name.to_string()).or_default().as_mut()
    }

    /// Creates a new empty conversation and returns the name
    ///
    /// # Arguments
    ///
    /// * `parameters`:
    ///
    /// returns: Result<String, RustGPTError>
    pub async fn new_conversation(&mut self, parameters: ConversationParameters) -> Result<String, RustGPTError> {
        // Create conversation and save
        let name = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
        let mut conversation = Conversation::new(parameters);
        self.save_conversation(&name, &mut conversation).await?;

        // Insert into the map
        self.conversations.insert(name.clone(), Some(conversation));
        Ok(name)
    }

    /// Saves the conversation to disk. Updates the conversation to mark `updated` as false.
    ///
    /// # Arguments
    ///
    /// * `name`: Name of the conversation (timestamp)
    /// * `conversation`: Conversation to save
    ///
    /// returns: Result<(), RustGPTError>
    async fn save_conversation(&self, name: &str, conversation: &mut Conversation) -> Result<(), RustGPTError> {
        // Serialize the conversation
        let path = self.base_path().join(format!("{}{}.yaml", Self::CONVERSATION_PREFIX, name));
        let Ok(content) = serde_yaml::to_string(&conversation) else {
            return Err(RustGPTError::SerializeConversation(name.to_string()));
        };

        // Save to disk
        let Ok(_) = fs::write(path, content).await else {
            return Err(RustGPTError::WriteConversation(name.to_string()));
        };
        conversation.updated = false;

        Ok(())
    }
}
