use thiserror::Error;
use tokio::io;

#[cfg(test)]
mod test_util;

/// Contains all the elements to render a TUI for interacting with the application.
pub mod tui;

/// Contains the related classes for handling conversations and completions with ChatGPT.
pub mod conversations;

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

pub type Result<T> = core::result::Result<T, RustGPTError>;

