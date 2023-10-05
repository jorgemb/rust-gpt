use rust_gpt::tui::Application;

#[tokio::main]
pub async fn main() -> rust_gpt::tui::Result<()>{
    // Create logger
    log4rs::init_file("config/rust4rs.yaml" , Default::default())
        .expect("Create error file");

    // Create application and run
    let app = Application::default();
    Application::start(app).await
}