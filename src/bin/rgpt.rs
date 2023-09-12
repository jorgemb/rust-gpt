use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use tabled::builder;
use tabled::settings::{Modify, Width};
use tabled::settings::object::Columns;
use tabled::settings::width::Wrap;

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
    Show(ShowConversation),
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

#[derive(Args, Debug)]
struct ShowConversation {
    path: PathBuf,

    #[arg(short = 'n', long)]
    conversation_index: Option<u16>,
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

/// Shows a conversation with the given index
async fn show_conversation(params: ShowConversation) {
    // Load the conversation
    let conversation = Conversation::load(params.path).await
        .expect("load conversation");

    // Get all the latest messages
    let latest = conversation.get_latest_messages();

    if let Some(index) = params.conversation_index {
        let Some(message) = latest.get(index as usize) else {
            eprintln!("No conversation with index {}", index);
            return;
        };

        // Get conversation anchored by the given message
        let message_list = conversation.get_message_list(Some(message.id()))
            .expect("get message list");

        // Show specific conversation
        let mut table_builder = builder::Builder::default();
        table_builder.set_header(["#", "CONTENT"]);
        for (i, msg) in message_list.iter().enumerate(){
            table_builder.push_record([format!("{}", i), msg.content().to_string()]);
        }
        let mut table = table_builder.build();

        // TODO: Calculate the width of the terminal for this width
        table.with(Modify::list(Columns::last(), Wrap::new(100)));
        println!("{}", table);

    } else {
        // Show all of the latest messages
        let mut table_builder = builder::Builder::default();
        table_builder.set_header(["INDEX", "LAST RESPONSE"]);
        for (i, msg) in latest.iter().enumerate(){
            table_builder.push_record([i.to_string(), msg.content().to_string()]);
        }
        let mut table = table_builder.build();

        // TODO: Calculate the width of the terminal for this width
        table.with(Modify::list(Columns::last(), Wrap::new(100)));
        println!("{}", table);
    }
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    match args.command {
        Commands::New(params) => new_conversation(params).await,
        Commands::Complete(params) => complete_conversation(params).await,
        Commands::Show(params) => show_conversation(params).await,
    }
}