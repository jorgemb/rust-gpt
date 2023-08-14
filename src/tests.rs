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
    let valid = vec!["conversation_123456.yaml", "conversation_20230814092231.yaml", "conversation_2023.yaml"];
    let invalid = vec!["conversation_.yaml", "other.yaml", "conversation_234234.txt", "random.txt"];

    for filename in valid.iter().chain(invalid.iter()){
        fs::write(path.join(filename), "").await.expect("Couldn't write file");
    }

    // Check that the test manager can find the valid paths
    let manager = ConversationManager::build(path)
        .await.expect("Couldn't create conversation manager");

    assert!(!manager.conversations.is_empty());
    assert_eq!(manager.conversations.len(), valid.len());

    for element in manager.conversations.iter(){
        assert!(valid.contains(&element.as_str()));
        assert!(!invalid.contains(&element.as_str()));
    }
}