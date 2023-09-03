use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::Terminal;
use ratatui::widgets::{Block, Borders, Paragraph};
use thiserror::Error;
use tokio::{io, join, time};
use tokio::sync::mpsc;

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
    Heartbeat,
}

/// Represents a basic application running conversations.
pub struct Application {
    // Signal if the application should keep running or not
    keep_running: bool,

    terminal: Option<Terminal<CrosstermBackend<std::io::Stdout>>>,
}

impl Application {
    pub async fn start(mut app: Application) -> Result<()>{
        let (tx, rx) = mpsc::channel(32);

        let input_handle = tokio::task::spawn_blocking(move || {
            Self::handle_input(tx)
        });
        let render_handle = tokio::task::spawn(async move {
            app.run(rx).await
        });

        let _ = join!(render_handle, input_handle);

        Ok(())
    }

    /// Runs the application in async mode
    async fn run(&mut self, mut messages: mpsc::Receiver<ApplicationMessage>) -> Result<()> {
        self.setup_terminal()?;

        while self.keep_running {
            // Render
            self.render().await?;

            // Handle messages
            if let Some(msg) = messages.recv().await{
                match msg{
                    ApplicationMessage::Quit => self.keep_running = false,
                    ApplicationMessage::Heartbeat => self.update()?,
                }
            }
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

        terminal.draw(|frame| {
            // Create the layout
            let layout_l1 = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(3),
                    ].as_ref()
                )
                .split(frame.size());
            let frame_menu = layout_l1.get(0).unwrap();
            let frame_status = layout_l1.get(2).unwrap();

            let layout_l2 = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(
                    [
                        Constraint::Percentage(20),
                        Constraint::Percentage(80),
                    ].as_ref()
                )
                .split(layout_l1[1]);
            let frame_list = layout_l2.get(0).unwrap();

            let layout_l3 = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Percentage(80),
                        Constraint::Percentage(20)
                    ].as_ref()
                )
                .split(layout_l2[1]);
            let frame_display = layout_l3.get(0).unwrap();
            let frame_input = layout_l3.get(1).unwrap();

            // Create the menu with title
            let menu = Paragraph::new("CHAT")
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title("RustGPT")
                    .title_alignment(Alignment::Center));
            frame.render_widget(menu, *frame_menu);

            // Create list
            let list = Block::default()
                .title("Conversations")
                .borders(Borders::ALL);
            frame.render_widget(list, *frame_list);

            // Create display
            let display = Block::default()
                .title("Display")
                .borders(Borders::ALL);
            frame.render_widget(display, *frame_display);

            // Create input
            let input = Block::default()
                .title("Input")
                .borders(Borders::ALL);
            frame.render_widget(input, *frame_input);

            // Create status
            let status = Paragraph::new("<Status1>\n<Status2>")
                .block(Block::default()
                    .borders(Borders::TOP));
            frame.render_widget(status, *frame_status);
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


    fn handle_input(sender: mpsc::Sender<ApplicationMessage>) -> Result<()> {
        let mut start_time = time::Instant::now();
        let heartbeat_duration = time::Duration::from_millis(100);
        loop {
            // Poll input
            if event::poll(heartbeat_duration)? {
                match event::read()? {
                    Event::FocusGained => {}
                    Event::FocusLost => {}
                    Event::Key(key) => {
                        match key.code {
                            KeyCode::Esc => {
                                sender.blocking_send(ApplicationMessage::Quit)?;
                                break;
                            },
                            _ => {}
                        }
                    }
                    Event::Mouse(_) => {}
                    _ => {}
                }
            }

            // Send heartbeat
            if start_time.elapsed() >= heartbeat_duration {
                sender.blocking_send(ApplicationMessage::Heartbeat)?;
                start_time = time::Instant::now();
            }
        }

        Ok(())
    }
    fn update(&self) -> Result<()> {
       Ok(())
    }
}

impl Default for Application {
    /// Returns a default application
    fn default() -> Self {
        Application {
            terminal: None,
            keep_running: true,
        }
    }
}
