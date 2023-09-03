use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use rust_gpt::conversations::{CompletionParametersBuilder, Conversation, create_chat_client};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    // Creates a new conversation
    New(NewConversation),
    Complete(CompleteConversation),
}

#[derive(Args, Debug)]
struct NewConversation {
    path: PathBuf,
    name: String,
    system_query: String,

    #[arg(short = 'n', long, default_value_t = 512)]
    max_tokens: u16,

    #[arg(short, long, default_value_t = 1.0)]
    temperature: f32,
}

#[derive(Args, Debug)]
struct CompleteConversation {
    path: PathBuf,
    query: String,
}

/// Creates a new conversation with the given parameters
///
/// # Arguments
///
/// * `conversation_params`:
///
/// returns: ()
async fn new_conversation(conversation_params: NewConversation) {
    // Create parameters
    let parameters = CompletionParametersBuilder::default()
        .temperature(conversation_params.temperature)
        .max_tokens(conversation_params.max_tokens)
        .build().expect("build parameters");

    // Create conversation
    let path = conversation_params.path.clone();

    let mut conversation = Conversation::build(
        parameters,
        conversation_params.path,
        conversation_params.system_query.as_str(),
    ).expect("build conversation");
    conversation.set_name(conversation_params.name);

    // Save the conversation
    conversation.save().await
        .expect("save conversation");

    println!("Conversation saved at: {}", path.display());
}

/// Tries to complete a conversation from the disk
///
/// # Arguments
///
/// * `params`:
///
/// returns: ()
async fn complete_conversation(params: CompleteConversation) {
    // Load the conversation
    let mut conversation = Conversation::load(params.path).await
        .expect("load conversation");

    // Get main conversation
    let &latest = conversation.get_latest_messages().first()
        .expect("No latest message in the conversation");

    // Add the query
    let &messages = conversation.add_queries(latest.id(), vec![params.query])
        .expect("create message")
        .first()
        .expect("first message created");

    // Create client
    let client = create_chat_client();

    // Complete the conversation
    let message_id = messages.id();
    let &completion = conversation.do_completion(message_id, client, None)
        .await
        .expect("complete conversation")
        .first()
        .expect("first response");

    // Show to the user
    // TODO: Show full conversation
    println!("Response: {}", completion.content());

    // Save the conversation
    conversation.save().await.expect("save conversation");
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    match args.command {
        Commands::New(params) => new_conversation(params).await,
        Commands::Complete(params) => complete_conversation(params).await,
    }
}