pub mod events;
pub mod views;

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use crate::queue::Database;

pub async fn run(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = views::AppState::new();

    loop {
        terminal.draw(|f| views::render(f, db, &state))?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => state.next_panel(),
                    KeyCode::Char('j') | KeyCode::Down => state.next_item(),
                    KeyCode::Char('k') | KeyCode::Up => state.prev_item(),
                    KeyCode::Char('r') => {
                        // TODO: retry selected failed item
                    }
                    KeyCode::Char('?') => state.toggle_help(),
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
