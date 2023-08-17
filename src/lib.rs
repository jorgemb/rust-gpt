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

    #[error("Error while working with filesystem")]
    DirectoryIO(#[from] io::Error),

    #[error("Error while serializing/deserializing")]
    Serialization(#[from] serde_yaml::Error),

    #[error("Couldn't write conversation {0}")]
    WriteConversation(String),

    #[error("Last message in the conversation is from user")]
    LastMessageFromUser(),

    #[error("Couldn't find conversation with name {0}")]
    ConversationNotFound(String),
}

type Result<T> = core::result::Result<T, RustGPTError>;


#[derive(Serialize, Deserialize, Builder)]
pub struct ConversationParameters {
    #[builder(default = "1.0")]
    temperature: f32,

    #[builder(default = "1")]
    n: u8,

    #[builder(default = "String::from(\"gpt-3.5-turbo\")")]
    model: String,

    #[builder(default = "String::from(\"You are a helpful assistant\")")]
    system_message: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: Role,
    pub content: String,
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
        Conversation {
            parameters,
            interactions: Vec::new(),
            updated: true,
        }
    }

    /// Adds a message to the conversation only if it is the first message, or the last one
    /// is a response from the server. This will mark the conversation as `updated`, signalling
    /// the manager that it should be saved in the disk.
    ///
    /// # Arguments
    ///
    /// * `message`: New message to add
    ///
    /// returns: Result<(), RustGPTError>
    ///
    /// # Examples
    ///
    /// ```
    /// use rust_gpt::{Conversation, ConversationParameters, ConversationParametersBuilder};
    /// let mut conversation = Conversation::new(ConversationParametersBuilder::default().build().unwrap());
    ///
    /// let res = conversation.add_message("What is the best way to write Rust code?");
    /// assert!(res.is_ok());
    ///
    /// let res = conversation.add_message("Whoops! Last message is from the user.");
    /// assert!(res.is_err());
    /// ```
    pub fn add_message(&mut self, message: &str) -> Result<()> {

        // Check if the last interaction is from the User
        if let Some(last_interaction) = self.interactions.last() {
            if last_interaction.role == Role::User {
                return Err(RustGPTError::LastMessageFromUser());
            }
        }

        // Add new message
        self.updated = true;
        let new_message = ConversationMessage { role: Role::User, content: message.to_string() };
        self.interactions.push(new_message);

        Ok(())
    }

    /// Returns the last response from the server.
    pub fn get_last_response(&self) -> Option<&str> {
        self.interactions.iter()
            .rev()
            .find(|r| r.role == Role::Assistant)
            .map(|r| r.content.as_str())
    }

    /// Returns the list of messages associated to the conversation.
    pub fn interactions(&self) -> &Vec<ConversationMessage> {
        &self.interactions
    }

    /// Returns if the conversation needs updating
    fn has_changed(&self) -> bool{
        self.updated
    }

    /// Mark the conversation as updated
    fn mark_updated(&mut self) {
        self.updated = false;
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
    pub async fn build<P>(path: P) -> Result<ConversationManager>
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
    pub async fn refresh_conversations(&mut self) -> Result<()> {
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

    /// Creates the path of a given Conversation
    ///
    /// # Arguments
    ///
    /// * `conversation`: Name (timestamp) of the conversation
    ///
    /// returns: PathBuf
    ///
    fn conversation_path(&self, conversation: &str) -> PathBuf {
        self.base_path.join(format!("{}{}.yaml", Self::CONVERSATION_PREFIX, conversation))
    }

    /// Returns the conversation with the given name
    pub async fn get_conversation(&mut self, name: &str) -> Result<&mut Conversation> {
        // Find conversation
        let path = self.conversation_path(name);
        let Some(conversation) = self.conversations.get_mut(name) else {
            return Err(RustGPTError::ConversationNotFound(name.to_string()));
        };

        // Check if the conversation has been loaded
        if conversation.is_none() {
            // Load conversation from disk
            let content = fs::read_to_string(path).await?;

            // Deserialize and update in the hashmap
            let loaded_conversation: Conversation = serde_yaml::from_str(&content)?;
            *conversation = Some(loaded_conversation);
        }

        Ok(conversation.as_mut().unwrap())
    }

    /// Creates a new empty conversation and returns the name
    ///
    /// # Arguments
    ///
    /// * `parameters`:
    ///
    /// returns: Result<String, RustGPTError>
    pub async fn new_conversation(&mut self, parameters: ConversationParameters) -> Result<String> {
        // Create conversation and save
        let name = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
        let mut conversation = Conversation::new(parameters);

        // Insert into the map
        self.conversations.insert(name.clone(), Some(conversation));

        // Save by name
        self.save_conversation(&name).await?;
        Ok(name)
    }

    /// Saves the conversation to disk. Updates the conversation to mark `updated` as false.
    ///
    /// # Arguments
    ///
    /// * `name`: Name of the conversation (timestamp)
    ///
    /// returns: Result<(), RustGPTError>
    async fn save_conversation(&mut self, name: &str) -> Result<()> {
        let path = self.conversation_path(name);

        // Serialize the conversation
        let conversation = self.get_conversation(name).await?;
        let content = serde_yaml::to_string(conversation)?;

        // Save to disk
        let Ok(_) = fs::write(path, content).await else {
            return Err(RustGPTError::WriteConversation(name.to_string()));
        };
        conversation.mark_updated();

        Ok(())
    }
}
