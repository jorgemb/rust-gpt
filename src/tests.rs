use crate::test_util::TempDirectoryHandler;

use super::*;

#[tokio::test]
async fn empty_manager_creation() {
    let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");

    let manager = ConversationManager::build(temp_dir.path())
        .await.expect("Couldn't create conversation manager");

    let conversations = manager.get_conversations().await.expect("Load conversations");
    assert!(conversations.is_empty());
}

#[tokio::test]
async fn manager_creation() {
    let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");

    // Create random files
    let path = temp_dir.path();
    let valid = vec!["123456.yaml", "20230814092231.yaml", "2023.yaml"];
    let invalid = vec!["conversation_234234.txt", "random.txt"];

    for filename in valid.iter().chain(invalid.iter()){
        fs::write(path.join(filename), "").await.expect("Couldn't write file");
    }

    // Check that the test manager can find the valid paths
    let manager = ConversationManager::build(path)
        .await.expect("Couldn't create conversation manager");

    let conversations = manager.get_conversations().await.expect("load conversations");

    assert!(!conversations.is_empty());
    assert_eq!(conversations.len(), valid.len());

    for element in conversations.iter(){
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
    let invalid_conversation = manager.load_conversation("does_not_exist.yaml").await;
    assert!(invalid_conversation.is_err());

    // Create new conversation
    let mut conversation = manager.new_conversation(
        ConversationParametersBuilder::default().build().expect("conversation builder")
    ).await.expect("new conversation");
    assert!(conversation.has_changed());
    assert!(!conversation.path.exists());

    // Save the conversation
    manager.save_conversation(&mut conversation).await.expect("save conversation");
    assert!(!conversation.has_changed());
    assert!(conversation.path.exists());

    // Load the conversation
    let mut conversation = manager.load_conversation(conversation.name()).await.expect("get conversation");
    assert!(!conversation.has_changed());
    assert_eq!(conversation.interactions.len(), 0);

    // Update the conversation
    let message = "What is the best way to conquer the World peacefully?";
    conversation.add_query(message)
        .expect("error writing message");

    assert!(conversation.has_changed());
    assert!(conversation.get_last_response().is_none());

    // Save the conversation
    manager.save_conversation(&mut conversation).await.expect("save conversation");
    assert!(conversation.path.exists());

    let conversation = manager.load_conversation(conversation.name()).await
        .expect("get conversation again");
    assert!(!conversation.has_changed());
}

#[tokio::test]
#[ignore]
async fn conversation_completion(){
    let temp_dir = TempDirectoryHandler::build().expect("temp directory");
    let mut manager = ConversationManager::build(temp_dir.path())
        .await.expect("Build manager");

    // Create new conversation
    let parameters = ConversationParametersBuilder::default()
        .build()
        .expect("conversation parameters");
    let mut conversation = manager.new_conversation(parameters)
        .await.expect("new conversation");

    // Add message and save
    conversation.add_query("A haiku about the KNM dutch exam.")
        .expect("write message");
    manager.save_conversation(&mut conversation).await.expect("save conversation");
    assert!(!conversation.has_changed(), "Conversation should not be changed after saving");

    conversation.do_completion().await.expect("complete conversation");
    println!("Message from OpenAI: {}", conversation.get_last_response().expect("get response"));
    assert!(conversation.has_changed(), "Conversation should be marked as change after completion");
}
