use rust_gpt::tui::Application;

#[tokio::main]
pub async fn main() -> rust_gpt::tui::Result<()>{
    // Create application and run
    let app = Application::default();
    Application::start(app).await
}