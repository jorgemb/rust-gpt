use crate::test_util::TempDirectoryHandler;

use super::*;

// #[tokio::test]
// async fn empty_manager_creation() {
//     let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");
//
//     let manager = ConversationManager::build(temp_dir.path())
//         .await.expect("Couldn't create conversation manager");
//
//     let conversations = manager.get_conversations().await.expect("Load conversations");
//     assert!(conversations.is_empty());
// }
//
// #[tokio::test]
// async fn manager_creation() {
//     let temp_dir = TempDirectoryHandler::build().expect("Couldn't create temp dir");
//
//     // Create random files
//     let path = temp_dir.path();
//     let valid = vec!["123456.yaml", "20230814092231.yaml", "2023.yaml"];
//     let invalid = vec!["conversation_234234.txt", "random.txt"];
//
//     for filename in valid.iter().chain(invalid.iter()){
//         fs::write(path.join(filename), "").await.expect("Couldn't write file");
//     }
//
//     // Check that the test manager can find the valid paths
//     let manager = ConversationManager::build(path)
//         .await.expect("Couldn't create conversation manager");
//
//     let conversations = manager.get_conversations().await.expect("load conversations");
//
//     assert!(!conversations.is_empty());
//     assert_eq!(conversations.len(), valid.len());
//
//     for element in conversations.iter(){
//         assert!(valid.contains(&element.as_str()));
//         assert!(!invalid.contains(&element.as_str()));
//     }
// }

// #[tokio::test]
// async fn conversations_creation_with_manager() {
//     let temp_dir = TempDirectoryHandler::build().expect("create temp dir");
//     let mut manager = ConversationManager::build(temp_dir.path())
//         .await.expect("manager creation");
//
//     // Get invalid conversation
//     let invalid_conversation = manager.load_conversation("does_not_exist.yaml").await;
//     assert!(invalid_conversation.is_err());
//
//     // Create new conversation
//     let mut conversation = manager.new_conversation(
//         ConversationParametersBuilder::default().build().expect("conversation builder")
//     ).await.expect("new conversation");
//     assert!(conversation.has_changed());
//     assert!(!conversation.path.exists());
// }

#[tokio::test]
async fn conversation_operations() {
    let temp_dir = TempDirectoryHandler::build().expect("temp dir");
    let path = temp_dir.path().join("test.yaml");

    // Create conversation
    let parameters = CompletionParametersBuilder::default().build()
        .expect("default parameters");
    let system_message = "You are a helpful assistant";
    let mut conversation = Conversation::build(parameters, path, system_message)
        .expect("basic conversation");

    // .. check name
    let conversation_name = "Test conversation";
    conversation.set_name(conversation_name.to_string());
    assert_eq!(conversation.name(), conversation_name);

    // .. root message
    let root_message = conversation.get_root_message();
    assert_eq!(root_message.role, Role::System);
    assert_eq!(&root_message.content, system_message);

    let root_message_id = root_message.id;

    // Add messages
    let queries = vec![String::from("Query1"), String::from("Query2"), String::from("Query3")];
    let added_messages = conversation.add_queries(root_message_id, queries.clone())
        .expect("add messages");
    for (idx, (q, m)) in queries.iter().zip(added_messages.iter()).enumerate() {
        let idx = idx as u8 + 1;
        assert_eq!(&m.content, q, "Content should match");
        assert_eq!(m.index, idx, "Sibling index should match");
    }

    // Add completions
    let completions = vec![String::from("Completion1"), String::from("Completion2")];
    let first_message_id = added_messages.first().unwrap().id;
    conversation.add_children_to_message(first_message_id, completions, Role::Assistant)
        .expect("Add completion messages");

    // Add more messages under same parent
    let added_messages = conversation.add_queries(root_message_id, queries.clone())
        .expect("add siblings");
    for (idx, (q, m)) in queries.iter().zip(added_messages).enumerate() {
        let idx = (idx + 1 + queries.len()) as u8;
        assert_eq!(&m.content, q, "Content should match");
        assert_eq!(m.index, idx, "Following sibling index should match");
    }

    // Get list of messages
    let message_list = conversation.get_message_list(None)
        .expect("message list");

    assert_eq!(message_list.len(), 3);
    let expected_content = vec![system_message, "Query1", "Completion1"];
    for (msg, expected) in message_list.iter().zip(expected_content){
        assert_eq!(msg.content.as_str(), expected);
    }
}

// #[tokio::test]
// #[ignore]
// async fn conversation_completion(){
//     let temp_dir = TempDirectoryHandler::build().expect("temp directory");
//     let mut manager = ConversationManager::build(temp_dir.path())
//         .await.expect("Build manager");
//
//     // Create new conversation
//     let parameters = ConversationParametersBuilder::default()
//         .build()
//         .expect("conversation parameters");
//     let mut conversation = manager.new_conversation(parameters)
//         .await.expect("new conversation");
//
//     // Trying a completion should give error
//     assert!(conversation.do_completion().await.is_err(), "No query has been given.");
//
//     // Add message and save
//     conversation.add_query("A small poem that highlights Rust language features.")
//         .expect("write message");
//     manager.save_conversation(&mut conversation).await.expect("save conversation");
//     assert!(!conversation.has_changed(), "Conversation should be marked as not changed after saving");
//
//     conversation.do_completion().await.expect("complete conversation");
//     println!("Message from OpenAI: {}", conversation.get_last_response().expect("get response"));
//     assert!(conversation.has_changed(), "Conversation should be marked as change after completion");
//
//     // Another completion should result in error
//     assert!(conversation.do_completion().await.is_err(), "No query has been given. Last response from System.");
// }
