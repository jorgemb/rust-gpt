use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use async_openai::config::OpenAIConfig;
use async_openai::types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequestArgs, Role};
use derive_builder::Builder;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use tokio::fs;
use uuid::Uuid;

use crate::{Result, RustGPTError};
use crate::RustGPTError::BadMessage;

/// Module with tests related to Conversations
#[cfg(test)]
mod tests;

/// Represents the different models that are available for doing chat completions. More details
/// can be found in the [official OpenAI documentation](https://platform.openai.com/docs/models/model-endpoint-compatibility).
#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Debug)]
pub enum CompletionModel {
    GPT35,
    GPT35_16K,
    GPT4,
    GPT4_32K,
}

impl ToString for CompletionModel {
    fn to_string(&self) -> String {
        match self {
            CompletionModel::GPT35 => "gpt-3.5-turbo",
            CompletionModel::GPT35_16K => "gpt-3.5-turbo-16k",
            CompletionModel::GPT4 => "gpt-4",
            CompletionModel::GPT4_32K => "gpt-4-32k",
        }.to_string()
    }
}


/// Represents the Conversation parameters for advancing the conversation with ChatGPT.
/// Each completion could contain different parameters within the same conversation.
///
/// Example
/// ```
/// use rust_gpt::conversations::{CompletionModel, CompletionParameters, CompletionParametersBuilder};
/// let parameters = CompletionParametersBuilder::default().build().expect("default build");
/// assert_eq!(parameters.temperature(), 1.0);
/// assert_eq!(parameters.n(), 1);
/// assert_eq!(parameters.model(), CompletionModel::GPT35);
/// assert_eq!(parameters.max_tokens(), 512);
///
/// // Temperature should be 0.0 <= x <= 2.0
/// let bad_parameters = CompletionParametersBuilder::default().temperature(2.1).build();
/// assert!(bad_parameters.is_err())
///
/// ```
#[derive(Debug, Serialize, Deserialize, Builder, Clone, PartialEq)]
#[builder(build_fn(validate = "Self::validate"))]
pub struct CompletionParameters {
    #[builder(default = "1.0")]
    temperature: f32,

    #[builder(default = "1")]
    n: u8,

    #[builder(default = "CompletionModel::GPT35")]
    model: CompletionModel,

    #[builder(default = "512")]
    max_tokens: u16,
}

impl CompletionParameters {
    pub fn temperature(&self) -> f32 { self.temperature }
    pub fn n(&self) -> u8 { self.n }
    pub fn model(&self) -> CompletionModel { self.model }
    pub fn max_tokens(&self) -> u16 { self.max_tokens }

    pub fn with_n(&self, n: u8) -> Self {
        let mut copy = self.clone();
        copy.n = n;

        copy
    }
}

impl CompletionParametersBuilder {
    /// Validates if the completion parameters are ok
    fn validate(&self) -> core::result::Result<(), String> {
        // Temperature 0.0 <= x <= 2.0
        if let Some(temperature) = self.temperature {
            match temperature {
                i if i < 0.0 => Err("Temperature must be >0.0".to_string()),
                i if i > 2.0 => Err("Temperature must be <2.0".to_string()),
                _ => Ok(())
            }
        } else {
            Ok(())
        }
    }
}

/// Represents a single message interaction with ChatGPT
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Message {
    /// Unique ID to identify the conversation
    id: Uuid,

    /// Unique ID of the message that goes before
    parent_id: Option<Uuid>,

    /// Index of the message (if it has sibling messages)
    index: u8,

    /// Role of the message (Assistant, User, Sytem...)
    role: Role,

    /// Actual message
    content: String,
}


impl Message {
    /// Creates a new message with a UUID as identifier.
    ///
    /// # Arguments
    ///
    /// * `role`: Role for the message
    /// * `content`: Content cannot be empty
    /// * `parent_id`: Parent conversation. Can be empty only when the Role==System
    ///
    /// returns: Message
    fn build(role: Role, content: String, parent_id: Option<Uuid>, sibling: Option<&Message>) -> Result<Self> {
        // Check content
        if content.is_empty() {
            return Err(BadMessage("Message must have a content".to_string()));
        }

        if parent_id.is_none() && role != Role::System {
            return Err(BadMessage("Parent can only be None when the role is System".to_string()));
        }

        // Check sibling
        let index = match sibling {
            None => 1u8,
            Some(sibling) => sibling.index + 1
        };

        let id = Uuid::new_v4();
        Ok(Message {
            id,
            parent_id,
            index,
            role,
            content,
        })
    }
    pub fn index(&self) -> u8 { self.index }
    pub fn role(&self) -> &Role { &self.role }
    pub fn content(&self) -> &String { &self.content }
    pub fn id(&self) -> Uuid { self.id }
}

/// Represents a Conversation with OpenAI, with initial parameters and
/// all interactions.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    default_parameters: CompletionParameters,
    interactions: HashMap<Uuid, Message>,

    /// Name of the conversation
    name: String,

    /// Path to where the file is stored
    #[serde(skip)]
    path: PathBuf,
}

impl Conversation {
    /// Creates a new Conversation object with the provided parameters. The conversation has
    /// a path but hasn't been stored in the filesystem yet.
    ///
    /// # Arguments
    ///
    /// * `parameters`: Conversation parameters
    /// * `path`: Path to where the `Conversation` is being stored.
    /// * `system_message`: Starting message for the conversation (given to the "System"). Cannot
    /// be emtpy.
    ///
    /// returns: Conversation
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use rust_gpt::conversations::{CompletionModel, CompletionParametersBuilder, Conversation};
    /// let parameters = CompletionParametersBuilder::default()
    ///     .n(1)
    ///     .model(CompletionModel::GPT4)
    ///     .build()
    ///     .unwrap();
    ///
    /// let mut conversation = Conversation::build(parameters, PathBuf::new(), "You are a helpful assistant")
    /// .expect("build conversation");
    /// ```
    pub fn build(parameters: CompletionParameters, path: PathBuf, system_message: &str) -> Result<Self> {
        // Create a system message
        let system_message = Message::build(
            Role::System,
            system_message.to_string(),
            None,
            None)?;

        // Create initial interactions
        let mut interactions = HashMap::new();
        interactions.insert(system_message.id, system_message);

        Ok(Conversation {
            default_parameters: parameters,
            interactions,
            path,
            name: String::new(),
        })
    }

    /// Returns the messages that are the latest response of a chain of messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use async_openai::types::Role;
    /// use rust_gpt::conversations::{CompletionParametersBuilder, Conversation};
    /// let parameters = CompletionParametersBuilder::default().build().expect("parameters");
    /// let system_message = "You are a helpful assistant";
    /// let conversation = Conversation::build(parameters, PathBuf::new(), system_message)
    ///     .expect("build conversation");
    ///
    /// let latest_messages = conversation.get_latest_messages();
    /// assert_eq!(latest_messages.len(), 1, "Only system message is the latest message");
    ///
    /// let message = latest_messages.first().expect("Single message");
    /// assert_eq!(*message.role(), Role::System);
    /// assert_eq!(message.content(), system_message);
    pub fn get_latest_messages(&self) -> Vec<&Message> {
        // Gather all the IDs that are a parent of another message
        let mut parents = HashSet::new();
        parents.extend(self.interactions.iter()
            .filter_map(|(_, m)| m.parent_id));

        // Find all the messages that are not the parent of another message
        self.interactions.iter()
            .filter_map(|(id, m)| if !parents.contains(id) { Some(m) } else { None })
            .collect()
    }

    /// Returns the list of messages that are part of a conversation. If an anchor is given,
    /// then the tree is searched so that message appears in the conversation.
    pub fn get_message_list(&self, anchor_message_id: Option<Uuid>) -> Result<Vec<&Message>> {
        let anchor = if let Some(msg_id) = anchor_message_id {
            if let Some(msg) = self.interactions.get(&msg_id) {
                msg
            } else {
                return Err(RustGPTError::MessageNotPartOfConversation);
            }
        } else {
            // Use the root as anchor
            self.get_root_message()
        };

        // Calculate parents
        let mut messages = vec![anchor];

        let mut current = anchor;
        while let Some(parent_id) = current.parent_id {
            let parent = self.interactions.get(&parent_id).unwrap();
            messages.push(parent);

            current = parent;
        }

        // .. reverse order so it is [root .. anchor]
        messages.reverse();

        // Calculate children
        let mut current = anchor.id;
        loop {
            let children = self.get_children(current);
            if children.is_empty() {
                break;
            } else {
                let first = children[0];
                current = first.id;
                messages.push(first);
            }
        }

        Ok(messages)
    }

    /// Returns the root system message
    fn get_root_message(&self) -> &Message {
        self.interactions.iter()
            .filter_map(|(_, msg)| match msg.parent_id {
                Some(_) => None,
                _ => Some(msg)
            })
            .next()
            .expect("there should always be a root message")
    }

    /// Returns all children of a given message
    fn get_children(&self, parent_message_id: Uuid) -> Vec<&Message> {
        let mut ret = self.interactions.iter()
            .filter_map(|(_, msg)|
                match msg.parent_id {
                    Some(parent_id) if parent_id == parent_message_id => Some(msg),
                    _ => None
                })
            .collect::<Vec<&Message>>();

        ret.sort_by_key(|msg| msg.index);
        ret
    }

    /// Returns the siblings of a message, including the calling message
    pub fn get_message_siblings(&self, message_id: Uuid) -> Result<Vec<&Message>> {
        // Validate message id
        let Some(message) = self.interactions.get(&message_id) else {
            return Err(RustGPTError::MessageNotPartOfConversation);
        };

        match message.parent_id {
            None => Ok(vec![message]),
            Some(id) => Ok(self.get_children(id)),
        }
    }

    /// Adds multiple queries to a parent message. The parent message should have a
    /// role of Assistant or System
    ///
    /// # Arguments
    ///
    /// * `parent_message`: Parent message with Assistant or System role
    /// * `queries`: List of queries to add
    ///
    /// returns: Result<Vec<&Message, Global>, RustGPTError> : List of queries
    pub fn add_queries(&mut self, parent_id: Uuid, queries: Vec<String>) -> Result<Vec<&Message>> {
        // Validate that the message exists within the conversation
        let Some(parent_message) = self.interactions.get(&parent_id) else {
            return Err(RustGPTError::MessageNotPartOfConversation);
        };

        // Validate that the parent message is assistant or role
        if !(parent_message.role == Role::Assistant || parent_message.role == Role::System) {
            return Err(RustGPTError::InvalidMessageRole);
        }

        let added_messages = self.add_children_to_message(parent_id, queries, Role::User)?;

        // Request the list of messages
        Ok(added_messages.into_iter()
            .map(|id| self.interactions.get(&id).unwrap())
            .collect())
    }

    /// Adds children to the given parent message. Validations is expected to have
    /// happened for message roles.
    fn add_children_to_message(&mut self, parent_id: Uuid, messages: Vec<String>, role: Role) -> Result<Vec<Uuid>> {
        // Get oldest sibling, if any
        let mut oldest_sibling = match self.get_children(parent_id).last() {
            Some(&sibling) => Some(sibling),
            _ => None,
        };

        let mut added_messages = Vec::with_capacity(messages.len());

        // Create the messages
        for msg in messages.into_iter() {
            let message = Message::build(
                role.clone(),
                msg,
                Some(parent_id),
                oldest_sibling)?;

            added_messages.push(message);
            oldest_sibling = added_messages.last();
        }

        // Add the messages to the interactions
        let message_ids: Vec<_> = added_messages.iter()
            .map(|msg| msg.id)
            .collect();

        self.interactions.extend(
            added_messages.into_iter()
                .map(|msg| (msg.id, msg))
        );

        Ok(message_ids)
    }

    /// Performs completions for the given message id
    pub async fn do_completion(&mut self, message_id: Uuid, client: ClientRef, n_completions: Option<u8>)
                               -> Result<Vec<&Message>> {

        // Validate that the given message is a user message
        let Some(message) = self.interactions.get(&message_id) else {
            error!("Trying to add a message to a conversation that doesn't exist");
            return Err(RustGPTError::MessageNotPartOfConversation);
        };

        if message.role != Role::User {
            error!("Role can only be User");
            return Err(RustGPTError::InvalidMessageRole);
        };

        // Get the trailing messages
        let mut messages = Vec::new();

        let mut current_msg = message;
        while let Some(parent_id) = current_msg.parent_id {
            messages.push(current_msg);

            if let Some(parent_msg) = self.interactions.get(&parent_id) {
                current_msg = parent_msg;
            } else {
                return Err(RustGPTError::MessageNotPartOfConversation);
            }
        }
        messages.push(current_msg);

        // Reverse the order
        messages.reverse();

        // Create the completions with the client
        let parameters = if let Some(n) = n_completions {
            self.default_parameters.with_n(n)
        } else {
            self.default_parameters.clone()
        };

        let completion_request = CreateChatCompletionRequestArgs::default()
            .n(parameters.n)
            .model(parameters.model.to_string())
            .max_tokens(parameters.max_tokens)
            .temperature(parameters.temperature)
            .messages(messages.iter().map(|msg| ChatCompletionRequestMessageArgs::default()
                .role(msg.role.clone())
                .content(msg.content.clone()).build().unwrap())
                .collect::<Vec<_>>())
            .build()?;

        // Perform the completion request
        debug!("Sending request to ChatGPT");
        let completion = client.chat().create(completion_request).await?;
        debug!("Request sent to ChatGPT");
        let responses: Vec<_> = completion.choices.into_iter()
            .filter_map(|choice| choice.message.content)
            .collect();
        debug!("Response from ChatGPT: {:?}", responses);

        let added_id = self.add_children_to_message(message_id, responses, Role::Assistant)?;

        Ok(added_id.into_iter()
            .filter_map(|id| self.interactions.get(&id))
            .collect())
    }

    /// Returns the name of the conversation
    pub fn name(&self) -> &str {
        &self.name
    }


    /// Sets a new name for the conversation
    ///
    /// # Arguments
    ///
    /// * `name`: Name of the conversation
    ///
    /// returns: String -> Previous name
    pub fn set_name(&mut self, name: String) -> String {
        let mut name = name;

        std::mem::swap(&mut name, &mut self.name);

        name
    }

    /// Tries to save the conversation to disk
    pub async fn save(&self) -> Result<()>{
        // Serialize
        let data = serde_yaml::to_string(self)?;

        // Save to the path
        fs::write(&self.path, data.as_bytes()).await?;

        Ok(())
    }

    /// Tries to load a conversation from disk
    ///
    /// # Arguments
    ///
    /// * `path`:
    ///
    /// returns: Result<Conversation, RustGPTError>
    pub async fn load<T>(path: T) -> Result<Self>
    where
        T: Into<PathBuf> + std::fmt::Debug
    {
        // Load file
        let path: PathBuf = path.into();
        let data = fs::read_to_string(&path).await?;

        // Deserialize conversation
        let mut conversation: Self = serde_yaml::from_str(&data)?;
        conversation.path = path;

        Ok(conversation)
    }

    /// Returns a depth-first iterator of the conversation
    pub fn iter(&self) -> ConversationIter{
        let mut current_stack = VecDeque::new();
        current_stack.push_front(self.get_root_message());
        
        ConversationIter{
            conversation: self,
            current_stack,
        }
    }
}

type ClientRef = Arc<async_openai::Client<OpenAIConfig>>;

/// Creates a new chat client
pub fn create_chat_client() -> ClientRef{
    Arc::new(async_openai::Client::new())
}

/// Allows depth first iteration over a conversation
pub struct ConversationIter<'a>{
    conversation: &'a Conversation,
    current_stack: VecDeque<&'a Message>,
}

impl<'a> Iterator for ConversationIter<'a>{
    type Item = &'a Message;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(current_message) = self.current_stack.pop_front() else {
            return None
        };

        // Get children
        let current_id = current_message.id;
        let children = self.conversation.get_children(current_id);
        for c in children.into_iter().rev(){
            self.current_stack.push_front(c);
        }

        Some(current_message)
    }
}