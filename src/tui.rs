use std::collections::VecDeque;
use std::path::PathBuf;

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::{KeyEventKind, KeyModifiers};
use ratatui::{Terminal, widgets};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::prelude::Modifier;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, ListState, Paragraph};
use thiserror::Error;
use tokio::{io, select, time};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

use crate::conversations::Conversation;

mod conversation_handler;

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

    #[error("RustGPT error")]
    ConversationError(#[from] crate::RustGPTError),
}

pub type Result<T> = std::result::Result<T, ApplicationError>;

/// Represents different messages that can be sent by the input
pub enum ApplicationMessage {
    /// Sent when the application is starting
    ApplicationStart,
    /// Quits the application
    Quit,
    /// Sent several times per second for updating
    Heartbeat,
    /// Loads conversations from disk and refreshes the list
    LoadConversations,

    // Manages conversations
    NextConversation,
    PreviousConversation,

    // Manages scrolling
    ScrollForward,
    ScrollBack,

    /// Shows a status message
    StatusMessage(String),
}

/// Represents a basic application running conversations.
pub struct Application {
    // Signal if the application should keep running or not
    keep_running: bool,

    terminal: Option<Terminal<CrosstermBackend<std::io::Stdout>>>,

    /// Message sender
    sender: Option<mpsc::Sender<ApplicationMessage>>,

    /// Status queue
    status_queue: VecDeque<String>,

    /// Status message time
    last_status_clear: Instant,

    // CONVERSATIONS
    /// Path were conversations will be loaded
    conversations_path: PathBuf,
    /// Current loaded conversations
    loaded_conversations: Vec<Conversation>,
    /// Conversation list state, used for TUI
    conversation_list_status: ListState,
    /// Amount of scrolling in the conversation
    conversation_scrolling: u16,
}

impl Application {
    pub async fn start(mut app: Application) -> Result<()> {
        let (tx, rx) = mpsc::channel(32);

        let input_tx = tx.clone();
        let input_handle = tokio::task::spawn_blocking(move || {
            Self::handle_input(input_tx)
        });

        let render_tx = tx.clone();
        let render_handle = tokio::task::spawn(async move {
            app.run(rx, render_tx).await
        });

        // Send the initial messages
        let _ = tx.send(ApplicationMessage::ApplicationStart).await;

        select! {
            _ = render_handle => {
                println!("Render finished");
            }
            _ = input_handle => {
                println!("Input finished");
            }
        }

        Ok(())
    }

    /// Runs the application in async mode
    async fn run(&mut self, mut messages: mpsc::Receiver<ApplicationMessage>, sender: mpsc::Sender<ApplicationMessage>) -> Result<()> {
        // Save the sender
        self.sender = Some(sender);

        self.setup_terminal()?;

        while self.keep_running {
            // Render
            self.render().await?;

            // Handle messages
            if let Some(msg) = messages.recv().await {
                match msg {
                    ApplicationMessage::Quit => self.keep_running = false,
                    ApplicationMessage::Heartbeat => self.update()?,
                    ApplicationMessage::ApplicationStart => {
                        // Refresh the conversations
                        if let Some(sender) = self.sender.as_ref() {
                            let _ = sender.send(ApplicationMessage::LoadConversations).await;
                        }
                    }
                    ApplicationMessage::LoadConversations => {
                        // Refresh the conversations
                        self.refresh_conversations().await;
                    }
                    ApplicationMessage::StatusMessage(msg) => {
                        self.status_queue.push_back(msg);
                    }
                    ApplicationMessage::NextConversation => {
                        let next_id = match self.conversation_list_status.selected() {
                            None => 0,
                            Some(id) => (id + 1) % self.loaded_conversations.len(),
                        };

                        self.conversation_list_status.select(Some(next_id));
                    }
                    ApplicationMessage::PreviousConversation => {
                        let previous_id = match self.conversation_list_status.selected() {
                            None => 0,
                            Some(id) => id.checked_sub(1).unwrap_or(self.loaded_conversations.len() - 1),
                        };

                        self.conversation_list_status.select(Some(previous_id));
                    }
                    ApplicationMessage::ScrollForward =>
                        self.conversation_scrolling = self.conversation_scrolling.saturating_add(1),

                    ApplicationMessage::ScrollBack =>
                        self.conversation_scrolling = self.conversation_scrolling.saturating_sub(1),
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
            let title = format!("ChatGPT {:?}", time::Instant::now());
            let menu = Paragraph::new("CHAT")
                .block(Block::default()
                    .borders(Borders::ALL)
                    // .title("RustGPT")
                    .title(title)
                    .title_alignment(Alignment::Center));
            frame.render_widget(menu, *frame_menu);

            // Create list
            let list_items: Vec<_> = self.loaded_conversations.iter()
                .map(|c| widgets::ListItem::new(c.name()))
                .collect();
            let list = widgets::List::new(list_items)
                .block(Block::default()
                    .title("Conversations")
                    .borders(Borders::ALL)
                ).highlight_style(
                Style::default()
                    .bg(Color::Gray)
                    .add_modifier(Modifier::BOLD));
            let mut list_status = self.conversation_list_status.clone();
            frame.render_stateful_widget(list, *frame_list, &mut list_status);

            // Create display
            let display_block = Block::default()
                .title("Display")
                .borders(Borders::ALL);
            if let Some(selected_id) = self.conversation_list_status.selected() {
                if let Some(conversation) = self.loaded_conversations.get(selected_id) {
                    let display = conversation_handler::conversation_widget(conversation, self.conversation_scrolling)
                        .unwrap_or(Paragraph::new("<ERROR>"))
                        .block(display_block);
                    frame.render_widget(display, *frame_display);
                }
            } else {
                let display = Paragraph::new("").block(display_block);
                frame.render_widget(display, *frame_display);
            }

            // Create input
            let input = Block::default()
                .title("Input")
                .borders(Borders::ALL);
            frame.render_widget(input, *frame_input);

            // Create status
            let default_status = String::new();
            let status_1 = self.status_queue.get(0).unwrap_or(&default_status);
            let status_2 = self.status_queue.get(1).unwrap_or(&default_status);
            let status = Paragraph::new(format!("{}\n{}", status_1, status_2))
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
        let mut start_time = Instant::now();
        let heartbeat_duration = Duration::from_millis(100);
        loop {
            // Poll input
            if event::poll(heartbeat_duration)? {
                match event::read()? {
                    Event::FocusGained => {}
                    Event::FocusLost => {}
                    Event::Key(key) => {
                        match (key.code, key.kind, key.modifiers) {
                            (KeyCode::Esc, _, _) => {
                                sender.blocking_send(ApplicationMessage::Quit)?;
                                break;
                            }
                            (KeyCode::Up, KeyEventKind::Press, KeyModifiers::NONE) => {
                                sender.blocking_send(ApplicationMessage::ScrollBack)?;
                            }
                            (KeyCode::Down, KeyEventKind::Press, KeyModifiers::NONE) => {
                                sender.blocking_send(ApplicationMessage::ScrollForward)?;
                            }
                            (KeyCode::Down, KeyEventKind::Press, KeyModifiers::SHIFT) => {
                                sender.blocking_send(ApplicationMessage::NextConversation)?;
                            }
                            (KeyCode::Up, KeyEventKind::Press, KeyModifiers::SHIFT) => {
                                sender.blocking_send(ApplicationMessage::PreviousConversation)?;
                            }
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

    /// Updates the status of the interface
    fn update(&mut self) -> Result<()> {
        const STATUS_CLEAR_TIME: Duration = Duration::from_secs(5);

        // Clear status if needed
        if self.status_queue.is_empty() {
            self.last_status_clear = Instant::now();
        } else if self.last_status_clear.elapsed() > STATUS_CLEAR_TIME {
            self.status_queue.pop_front();
            self.last_status_clear = Instant::now();
        }

        Ok(())
    }



    /// Loads the conversations from disk
    async fn refresh_conversations(&mut self) {
        // Load all conversations
        let loaded_conversations =
            conversation_handler::find_conversations(&self.conversations_path).await;

        let number_of_conversations = loaded_conversations.len();
        self.loaded_conversations = loaded_conversations;

        // Send message reporting update
        if let Some(sender) = self.sender.as_ref() {
            let _ = sender.send(
                ApplicationMessage::StatusMessage(format!("Loaded {} conversations", number_of_conversations))
            ).await;
        }

        // Reset selection
        self.conversation_list_status.select(Some(0));
    }
}

impl Default for Application {
    /// Returns a default application
    fn default() -> Self {
        // Default conversations path
        let conversations_path = PathBuf::from("conversations/");

        Application {
            terminal: None,
            keep_running: true,

            sender: None,

            status_queue: VecDeque::new(),
            last_status_clear: Instant::now(),

            conversations_path,
            loaded_conversations: Vec::new(),
            conversation_list_status: ListState::default(),
            conversation_scrolling: 0,
        }
    }
}
