use std::path::Path;
use crate::test_util::TempDirectoryHandler;

use super::*;

#[tokio::test]
async fn conversation_operations() {
    let temp_dir = TempDirectoryHandler::build().expect("temp dir");
    let path = temp_dir.path().join("test.yaml");

    // Create conversation
    let parameters = CompletionParametersBuilder::default().build()
        .expect("default parameters");
    let system_message = "You are a helpful assistant";
    let mut conversation = Conversation::build(parameters, path.clone(), system_message)
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
    for (msg, expected) in message_list.iter().zip(expected_content) {
        assert_eq!(msg.content.as_str(), expected);
    }

    // Get all the siblings of a message
    let siblings = conversation.get_message_siblings(first_message_id)
        .expect("get siblings");

    assert_eq!(siblings.len(), queries.len() * 2);
    for (id, s) in siblings.into_iter().enumerate() {
        let n = (id % 3) + 1;
        assert_eq!(s.content, format!("Query{}", n));
    }

    // Save the conversation
    conversation.save().await
        .expect("save conversation");

    // Load the conversation and compare
    let loaded_conversation = Conversation::load(&path).await
        .expect("load conversation");
    assert_eq!(conversation, loaded_conversation);
}

#[tokio::test]
#[ignore]
async fn conversation_completion() {
    let temp_dir = TempDirectoryHandler::build().expect("temp directory");
    let path = temp_dir.path().join("conversation.yml");

    // Create a new conversation
    let parameters = CompletionParametersBuilder::default()
        .n(2)
        .model(CompletionModel::GPT35)
        .max_tokens(128)
        .temperature(1.0)
        .build()
        .expect("parameters");

    let mut conversation = Conversation::build(
        parameters,
        path,
        "You are a helpful assistant that must provide answers in Spanish.")
        .expect("build conversation");

    // Add query
    let root_conversation_id = conversation.get_root_message().id;

    let queries = vec![String::from("What is the greatest thing that has come out of ChatGPT")];
    let message_id = conversation.add_queries(root_conversation_id, queries).expect("add queries")
        .first().expect("Single query").id;

    // Create a client
    let client = create_chat_client();

    // Do completion
    let completions = conversation.do_completion(message_id, client, None)
        .await
        .expect("perform completions");

    for c in completions {
        println!("{:?}", c);
    }
}

#[tokio::test]
async fn conversation_iter() {
    // Create the conversation
    let params = CompletionParametersBuilder::default().build()
        .expect("completion parameters");
    let mut conversation = Conversation::build(params, PathBuf::new(), "System")
        .expect("build conversation");

    // Add queries
    let root_id = conversation.get_root_message().id;
    let children = conversation.add_children_to_message(
        root_id,
        vec![String::from("Q1"), String::from("Q2")],
        Role::User)
        .expect("Add messages");

    let c1 = children[0];
    let c2 = children[1];

    let _ = conversation.add_children_to_message(
        c1,
        vec![String::from("A1"), String::from("A2")],
        Role::Assistant)
        .expect("add children");

    let children = conversation.add_children_to_message(
        c2,
        vec![String::from("A3"), String::from("A4")],
        Role::Assistant
    ).expect("add children");
    let a3 = children[0];

    let _ = conversation.add_children_to_message(
        a3,
        vec![String::from("Q3"), String::from("Q4")],
        Role::User
    ).expect("add children");

    // Expected run
    let expected_content = vec![
        String::from("System"),
        String::from("Q1"),
        String::from("A1"),
        String::from("A2"),
        String::from("Q2"),
        String::from("A3"),
        String::from("Q3"),
        String::from("Q4"),
        String::from("A4"),
    ];

    for (expected, message) in expected_content.iter().zip(conversation.iter()){
        assert_eq!(expected, &message.content, "Expected {} in message {:?}", expected, message);
    }
}