use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui::widgets::{Block, Borders};
use thiserror::Error;
use tokio::io;

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("IO Error")]
    IOError(#[from] io::Error),

    #[error("Error while processing input.")]
    InputError(#[from] tokio::sync::mpsc::error::SendError<ApplicationMessage>),

    #[error("Error on async task")]
    TokioError(#[from] tokio::task::JoinError),

    #[error("No terminal was created")]
    NoTerminal,

}

pub type Result<T> = std::result::Result<T, ApplicationError>;

/// Represents different messages that can be sent by the input
pub enum ApplicationMessage {
    /// Quits the application
    Quit,
}

/// Represents a basic application running conversations.
pub struct Application {
    // Signal if the application should keep running or not
    keep_running: bool,

    n: u64,

    // Backend terminal
    terminal: Option<Terminal<CrosstermBackend<std::io::Stdout>>>,
}

impl Application {
    /// Runs the application in async mode
    pub async fn run(&mut self) -> Result<()> {
        self.setup_terminal()?;

        while self.keep_running {
            self.render().await?;
            self.handle_input().await?;
        }

        self.restore_terminal()?;

        Ok(())
    }

    /// Renders the scene once
    async fn render(&mut self) -> Result<()> {
        // Get terminal
        let Some(terminal) = &mut self.terminal else {
            return Err(ApplicationError::NoTerminal);
        };

        // Draw the terminal
        self.n += 1;
        terminal.draw(|frame| {
            let greeting = Block::default()
                .title(format!("Hello from ChatGPT {}", self.n))
                .borders(Borders::ALL);

            frame.render_widget(greeting, frame.size());
        })?;

        Ok(())
    }

    fn setup_terminal(&mut self) -> Result<()> {
        let mut stdout = std::io::stdout();
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen)?;

        self.terminal = Some(Terminal::new(CrosstermBackend::new(stdout))?);
        Ok(())
    }

    fn restore_terminal(&mut self) -> Result<()> {
        if let Some(terminal) = self.terminal.as_mut().take() {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
            terminal.show_cursor()?;
        }

        Ok(())
    }


    async fn handle_input(&mut self) -> Result<()> {
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::FocusGained => {}
                Event::FocusLost => {}
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Esc => self.keep_running = false,
                        _ => {}
                    }
                }
                Event::Mouse(_) => {}
                _ => {}
            }
        }

        Ok(())
    }
}

impl Default for Application {
    /// Returns a default application
    fn default() -> Self {
        Application {
            terminal: None,
            keep_running: true,

            n: 0,
        }
    }
}
