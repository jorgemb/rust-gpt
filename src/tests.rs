use crate::test_util::TempDirectoryHandler;

use super::*;

#[tokio::test]
async fn empty_manager_creation() {
    let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");

    let manager = ConversationManager::build(temp_dir.path())
        .await.expect("Couldn't create conversation manager");
    assert!(manager.conversations.is_empty());
}

#[tokio::test]
async fn manager_creation() {
    let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");

    // Create random files
    let path = temp_dir.path();
    let valid = vec!["123456", "20230814092231", "2023"];
    let invalid = vec!["conversation_.yaml", "other.yaml", "conversation_234234.txt", "random.txt"];

    for filename in valid.iter().map(|&v| format!("conversation_{}.yaml", v)).chain(invalid.iter().map(|&v| v.to_string())){
        fs::write(path.join(filename), "").await.expect("Couldn't write file");
    }

    // Check that the test manager can find the valid paths
    let manager = ConversationManager::build(path)
        .await.expect("Couldn't create conversation manager");

    assert!(!manager.conversations.is_empty());
    assert_eq!(manager.conversations.len(), valid.len());

    for element in manager.conversations.keys(){
        assert!(valid.contains(&element.as_str()));
        assert!(!invalid.contains(&element.as_str()));
    }
}

#[tokio::test]
async fn conversations(){
    let temp_dir = TempDirectoryHandler::build().expect("create temp dir");
    let mut manager = ConversationManager::build(temp_dir.path())
        .await.expect("manager creation");

    // Get invalid conversation
    let invalid_conversation = manager.get_conversation("anything");
    assert!(invalid_conversation.is_none());

    // Create new conversation
    let name = manager.new_conversation(
        ConversationParametersBuilder::default().build().expect("conversation builder")
    ).await.expect("new conversation");

    assert!(name.starts_with(&chrono::Utc::now().format("%Y%m%d").to_string()));

    // Get the conversation
    let conversation = manager.get_conversation(&name).expect("get conversation");
    assert!(!conversation.updated);
    assert_eq!(conversation.interactions.len(), 0);

    let path = temp_dir.path().join(format!("{}{}.yaml", ConversationManager::CONVERSATION_PREFIX, name));
    assert!(path.exists());
}