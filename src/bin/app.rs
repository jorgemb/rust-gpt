use std::error::Error;
use rust_gpt::tui::Application;

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>>{
    // Create application and run
    let mut app = Application::default();
    Ok(app.run().await?)
}