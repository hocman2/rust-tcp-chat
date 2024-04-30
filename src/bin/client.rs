mod chat_message;

use rand::seq::IteratorRandom;
use tokio::{io::{self, AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::mpsc::{Receiver, Sender}};
use tokio::sync::mpsc;
use std::fs;
use chat_message::ChatMessage;
use std::error::Error;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

/// App holds the state of the application
struct App {
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    character_index: usize,
    /// History of recorded messages
    messages: Vec<String>,
    send_msg_tx: Sender<ChatMessage>,
    receive_msg_rx: Receiver<ChatMessage>,
    username: String
}

impl App {
    const fn new(send_msg_tx: Sender<ChatMessage>, receive_msg_rx: Receiver<ChatMessage>, username: String) -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            character_index: 0,
            send_msg_tx, receive_msg_rx, username
        }
    }

    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.character_index.saturating_sub(1);
        self.character_index = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.character_index.saturating_add(1);
        self.character_index = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        let index = self.byte_index();
        self.input.insert(index, new_char);
        self.move_cursor_right();
    }

    /// Returns the byte index based on the character position.
    ///
    /// Since each character in a string can be contain multiple bytes, it's necessary to calculate
    /// the byte index based on the index of the character.
    fn byte_index(&mut self) -> usize {
        self.input
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.character_index)
            .unwrap_or(self.input.len())
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.character_index != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.character_index;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.chars().count())
    }

    fn reset_cursor(&mut self) {
        self.character_index = 0;
    }

    fn receive_message(&mut self, message: ChatMessage) {
        self.messages.push(message.to_string());
    }

    fn submit_message(&mut self) {

        let message = ChatMessage {username: self.username.clone(), content: self.input.clone()};

        // Inner state and UI update
        self.messages.push(message.clone().to_string());
        self.input.clear();
        self.reset_cursor();

        // Send through the channel for the network task
        let tx = self.send_msg_tx.clone();
        tokio::spawn(async move {
            tx.send(message).await.unwrap();
        });
    }
}

// Generate a random name for this client
fn generate_name() -> String {
    let rand_adjective = fs::read_to_string("english-adjectives.txt").unwrap().lines().choose(&mut rand::thread_rng()).unwrap().to_string();
    let rand_noun = fs::read_to_string("nounlist.txt").unwrap().lines().choose(&mut rand::thread_rng()).unwrap().to_string();

    String::from(rand_adjective + "-" + rand_noun.as_str())
}

async fn run_network(receive_msg_tx: Sender<ChatMessage>, mut send_msg_rx: Receiver<ChatMessage>) {
    // Connect to the server
    let socket = TcpStream::connect("127.0.0.1:6969").await.unwrap();
        
    // Split the socket in two parts
    let (mut rd, mut wt) = io::split(socket);

    // Sending message task
    let write_t = tokio::spawn(async move {
        
        // Wait for send event from the UI
        while let Some(message) = send_msg_rx.recv().await {
            // Send input to the server
            wt.write(message.to_string().as_bytes()).await.unwrap();
            wt.flush().await.unwrap();
        }
    });

    // Receiving message task
    let read_t = tokio::spawn(async move {
        loop {
            let mut buffer = vec![0; 1024];
            let num_bytes = rd.read(&mut buffer).await.unwrap();
            if num_bytes > 0 {
                let as_str = String::from_utf8_lossy(&buffer[..num_bytes]).to_string();
                receive_msg_tx.send(ChatMessage::from(as_str)).await.unwrap();
            }
        }
    });

    write_t.await.unwrap();
    read_t.await.unwrap();
}

fn setup_app(send_msg_tx: Sender<ChatMessage>, receive_msg_rx: Receiver<ChatMessage>, username: String) -> Result<(Terminal<CrosstermBackend<std::io::Stdout>>, App), Box<dyn Error>> {
    enable_raw_mode()?;

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    // create app and run it
    Ok(( terminal, App::new(send_msg_tx, receive_msg_rx, username) ))
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match key.kind {
                KeyEventKind::Press => match key.code {
                    KeyCode::Enter => app.submit_message(),
                    KeyCode::Char(to_insert) => {
                        app.enter_char(to_insert);
                    }
                    KeyCode::Backspace => {
                        app.delete_char();
                    }
                    KeyCode::Left => {
                        app.move_cursor_left();
                    }
                    KeyCode::Right => {
                        app.move_cursor_right();
                    }
                    KeyCode::Esc => {
                        return Ok(());
                    }
                    _ => {}
                }
                _ => {}
            }
        }

        if let Ok(message) = app.receive_msg_rx.try_recv() {
            app.receive_message(message);
        }
    }

}

fn handle_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: App) -> Result<(), Box<dyn Error>> {
    let res = run_app(terminal, app);

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;

    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let vertical = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Min(1),
    ]);
    let [help_area, input_area, messages_area] = vertical.areas(f.size());

    let (msg, style) = (
        vec![
            "Press ".into(),
            "Esc".bold(),
            " to exit, ".into(),
            "Enter".bold(),
            " to send message".into(),
        ],
        Style::default(),
    );

    let text = Text::from(Line::from(msg)).patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, help_area);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, input_area);

    // Make the cursor visible and ask ratatui to put it at the specified coordinates after
    // rendering
    #[allow(clippy::cast_possible_truncation)]
    f.set_cursor(
        // Draw the cursor at the current position in the input field.
        // This position is can be controlled via the left and right arrow key
        input_area.x + app.character_index as u16 + 1,
        // Move one line down, from the border to the input line
        input_area.y + 1,
    );

    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .enumerate()
        .map(|(_, m)| {
            let content = Line::from(Span::raw(format!("{m}")));
            ListItem::new(content)
        })
        .collect();
    let messages =
        List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
    f.render_widget(messages, messages_area);
}

fn main() {

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Two channels to send data back and forth between UI and network tasks
    let (send_msg_tx, send_msg_rx) = mpsc::channel(2);
    let (receive_msg_tx, receive_msg_rx) = mpsc::channel(2);

    let username = generate_name();

    // Run network tasks
    let _network_task = rt.spawn(async move {
        run_network(receive_msg_tx, send_msg_rx).await; 
    });

    // Prevents program from ending prematurly
    // Setup and start UI
    let _app_task = rt.block_on(async move {
        match setup_app(send_msg_tx, receive_msg_rx, username) {
            Ok((mut terminal, app)) => {
                if let Err(e) = handle_app(&mut terminal, app) {
                    eprintln!("{}", e);
                }
            }
            Err(e) => eprintln!("{}", e)
        }
    });
}