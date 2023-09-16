use std::path::Path;

use ratatui::widgets::{Paragraph, Wrap};
use tokio::fs;

use crate::conversations::Conversation;
use crate::Result;

/// Loads all conversations in the given path.
///
/// # Arguments 
///
/// * `directory_path`: Path to the directory where to find the conversations
///
/// returns: Vec<Conversation, Global> 
///
pub async fn find_conversations<P>(directory_path: P) -> Vec<Conversation> where P: AsRef<Path> {
    let mut loaded_conversations = Vec::new();

    // Find all possible conversations in the given path
    if let Ok(mut directory_files) = fs::read_dir(directory_path).await {
        while let Ok(Some(current_file)) = directory_files.next_entry().await {
            // Check if the extension matches YAML
            let file_path = current_file.path();
            let Some(extension) = file_path.extension() else { continue; };
            if extension == "yaml" {
                // Try loading the Conversation file
                if let Ok(conversation) = Conversation::load(file_path).await {
                    loaded_conversations.push(conversation);
                }
            }
        }
    }

    loaded_conversations
}

/// Creates a Paragraph widget from a given conversation and scrolling value
///
/// # Arguments
///
/// * `conversation`:
/// * `scrolling`:
///
/// returns: Result<<unknown>, <unknown>>
///

pub fn conversation_widget(conversation: &Conversation, scrolling: u16) -> Result<Paragraph> {
    // Wrapping
    let wrap = Wrap { trim: false };

    // Parse messages
    let messages = conversation.get_message_list(None)?;
    let last_message = messages.last().unwrap();

    Ok(
        Paragraph::new(last_message.content().clone())
            .wrap(wrap)
            .scroll((scrolling, 0))
    )
}