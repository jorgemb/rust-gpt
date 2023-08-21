use std::path::PathBuf;
use std::sync::Arc;

use async_openai::config::OpenAIConfig;
use async_openai::types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, Role};
use derive_builder::Builder;
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

    #[error("Error interacting with OpenAI")]
    ClientError(#[from] async_openai::error::OpenAIError),

    #[error("Couldn't get an answer from the client: {0}")]
    ResponseError(String),

    #[error("Last message in the conversation is from user")]
    LastMessageFromUser(),

    #[error("Couldn't find conversation with name {0}")]
    ConversationNotFound(String),

    #[error("No client given to the Conversation")]
    NoClientSpecified,

    #[error("No query has been specified for the completion")]
    NoQueryGiven,
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

    #[builder(default = "256")]
    max_tokens: u16,

    #[builder(default = "String::from(\"You are a helpful assistant\")")]
    system_message: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ConversationMessage {
    pub role: Role,
    pub content: String,
}

/// Represents a Conversation with OpenAI, with initial parameters and
/// all interactions.
#[derive(Serialize, Deserialize)]
pub struct Conversation {
    parameters: ConversationParameters,
    interactions: Vec<ConversationMessage>,

    // Set to true when the conversation has been changed and needs to be saved to disk
    #[serde(skip)]
    updated: bool,

    // Path to where the file is stored
    #[serde(skip)]
    path: PathBuf,

    // Name of the conversation
    #[serde(skip)]
    name: String,

    // Client reference for own calls
    #[serde(skip)]
    client: Option<ClientRef>,
}

impl Conversation {
    /// Creates a new Conversation object with the provided parameters. The conversation has
    /// a path but hasn't been stored in the filesystem yet.
    ///
    /// # Arguments
    ///
    /// * `parameters`: Conversation parameters
    /// * `path`: Path to where the `Conversation` is being stored.
    ///
    /// returns: Conversation
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use rust_gpt::{Conversation, ConversationParametersBuilder};
    /// let parameters = ConversationParametersBuilder::default()
    ///     .n(1)
    ///     .model(String::from("gpt-3.5-turbo"))
    ///     .system_message(String::from("You are a helpful assistant."))
    ///     .build()
    ///     .unwrap();
    ///
    /// let mut conversation = Conversation::new(parameters, PathBuf::new(), None);
    /// assert!(conversation.has_changed());
    /// assert!(conversation.interactions().is_empty());
    ///
    /// conversation.add_query("Help me create a good ChatGPT rust library.")
    /// .expect("Add query");
    ///
    /// assert!(conversation.add_query("Second query should fail until a response is obtained")
    /// .is_err());
    /// ```
    pub fn new(parameters: ConversationParameters, path: PathBuf, client: Option<ClientRef>) -> Self {
        let name = path.file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string();

        Conversation {
            parameters,
            interactions: Vec::new(),
            updated: true,
            path,
            name,
            client,
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
    /// use std::path::PathBuf;
    /// use rust_gpt::{Conversation, ConversationParameters, ConversationParametersBuilder};
    /// let mut conversation = Conversation::new(ConversationParametersBuilder::default().build().unwrap(), PathBuf::new(), None);
    ///
    /// let res = conversation.add_query("What is the best way to write Rust code?");
    /// assert!(res.is_ok());
    ///
    /// let res = conversation.add_query("Whoops! Last message is from the user.");
    /// assert!(res.is_err());
    /// ```
    pub fn add_query(&mut self, message: &str) -> Result<()> {

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

    /// Perform a completion request
    pub async fn do_completion(&mut self) -> Result<()> {
        // Get a reference to the client
        let Some(client) = &self.client else {
            return Err(RustGPTError::NoClientSpecified);
        };

        // A completion can only be requested if the last message is a query
        if let Some(last_interaction) = self.interactions.last(){
            if last_interaction.role != Role::User{
                return Err(RustGPTError::NoQueryGiven)
            }
        } else {
            return Err(RustGPTError::NoQueryGiven)
        }

        // Create the request
        let Ok(request) = self.create_request() else {
            return Err(RustGPTError::NoClientSpecified);
        };

        // Send request to client
        let response = client.chat().create(request).await?;

        // Parse response
        if response.choices.len() > 1 {
            unimplemented!("Multiple choices are not implemented yet...");
        }

        if let Some(answer) = response.choices.into_iter().next() {
            self.interactions.push(
                ConversationMessage {
                    role: answer.message.role,
                    content: answer.message.content.unwrap_or_default(),
                }
            );
            self.updated = true;

            Ok(())
        } else {
            Err(RustGPTError::ResponseError("No choice in response".to_string()))
        }
    }

    /// Returns the list of messages associated to the conversation.
    pub fn interactions(&self) -> &Vec<ConversationMessage> {
        &self.interactions
    }

    /// Returns if the conversation needs updating
    pub fn has_changed(&self) -> bool {
        self.updated
    }

    /// Mark the conversation as updated
    fn mark_updated(&mut self) {
        self.updated = false;
    }

    /// Creates an OpenAI request
    fn create_request(&self) -> Result<CreateChatCompletionRequest> {
        let mut messages = Vec::with_capacity(self.interactions.len() + 1);
        messages.push(
            ChatCompletionRequestMessageArgs::default()
                .role(Role::System)
                .content(self.parameters.system_message.clone())
                .build()?
        );
        messages.extend(
            self.interactions.iter().cloned().map(
                |msg| ChatCompletionRequestMessageArgs::default()
                    .role(msg.role)
                    .content(msg.content)
                    .build().unwrap()
            )
        );

        let chat_completion = CreateChatCompletionRequestArgs::default()
            .n(self.parameters.n)
            .model(&self.parameters.model)
            .temperature(self.parameters.temperature)
            .max_tokens(self.parameters.max_tokens)
            .messages(messages)
            .build()?;

        Ok(chat_completion)
    }

    /// Returns the name of the conversation
    pub fn name(&self) -> &str {
        &self.name
    }
}

type ClientRef = Arc<async_openai::Client<OpenAIConfig>>;

/// Helps with creating, saving and loading conversations.
pub struct ConversationManager {
    base_path: PathBuf,

    // OpenAI client
    client: ClientRef,
}

impl ConversationManager {
    /// The ConversationManager helps managing the directory where all the conversations are stored.
    /// Helps discovering, managing and updating each of the files.
    ///
    /// This reads the `OPENAI_API_KEY` from the environmental variables to create the client.
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

        // Create client
        let client = Arc::new(async_openai::Client::new());

        // Create manager and refresh conversations
        let conversation_manager = ConversationManager {
            base_path,
            client,
        };

        Ok(conversation_manager)
    }

    pub fn base_path(&self) -> &PathBuf {
        &self.base_path
    }

    /// Returns a list of the conversations within the path
    pub async fn get_conversations(&self) -> Result<Vec<String>> {
        // Returns the name of all conversations within the path
        let mut entries = fs::read_dir(self.base_path()).await?;

        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.ends_with(".yaml") {
                    names.push(file_name.to_string());
                }
            }
        }

        Ok(names)
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
        self.base_path.join(conversation)
    }

    /// Returns the conversation with the given name
    pub async fn load_conversation(&mut self, name: &str) -> Result<Conversation> {
        // Find conversation
        let path = self.conversation_path(name);

        // Load conversation from disk
        let content = fs::read_to_string(&path).await?;

        // Deserialize and update
        let mut loaded_conversation: Conversation = serde_yaml::from_str(&content)?;
        loaded_conversation.client = Some(self.client.clone());
        loaded_conversation.path = path;
        loaded_conversation.name = name.to_string();
        loaded_conversation.mark_updated();

        Ok(loaded_conversation)
    }

    /// Creates a new empty conversation and returns the name
    ///
    /// # Arguments
    ///
    /// * `parameters`:
    ///
    /// returns: Result<String, RustGPTError>
    pub async fn new_conversation(&mut self, parameters: ConversationParameters) -> Result<Conversation> {
        // Create conversation and save
        let name = chrono::Utc::now().format("%Y%m%d%H%M%S.yaml").to_string();
        let path = self.base_path().join(name);
        let conversation = Conversation::new(parameters, path, Some(self.client.clone()));

        Ok(conversation)
    }

    /// Saves the conversation to disk. Updates the conversation to mark `updated` as false.
    ///
    /// # Arguments
    ///
    /// * `name`: Name of the conversation (timestamp)
    ///
    /// returns: Result<(), RustGPTError>
    pub async fn save_conversation(&mut self, conversation: &mut Conversation) -> Result<()> {
        // Check if the conversation has changed
        if !conversation.has_changed() {
            return Ok(());
        }

        // Serialize the conversation
        let content = serde_yaml::to_string(conversation)?;

        // Save to disk
        let Ok(_) = fs::write(&conversation.path, content).await else {
            return Err(RustGPTError::WriteConversation(conversation.path.display().to_string()));
        };
        conversation.mark_updated();

        Ok(())
    }
}
