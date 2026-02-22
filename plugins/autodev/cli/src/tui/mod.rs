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

use crate::config;
use crate::queue::Database;

use events::LogTailer;
use views::Panel;

const LOG_TAIL_MAX_LINES: usize = 200;

pub async fn run(db: &Database) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = views::AppState::new();

    // Initialize log tailer
    let home = config::autodev_home(&config::RealEnv);
    let log_dir = home.join("logs");
    let mut tailer = LogTailer::new(log_dir);
    state.log_lines = tailer.initial_load(LOG_TAIL_MAX_LINES);

    loop {
        // Poll for new log lines every render cycle
        let new_lines = tailer.poll_new_lines();
        if !new_lines.is_empty() {
            state.log_lines.extend(new_lines);
            // Keep buffer bounded
            if state.log_lines.len() > LOG_TAIL_MAX_LINES * 2 {
                let drain_count = state.log_lines.len() - LOG_TAIL_MAX_LINES;
                state.log_lines.drain(..drain_count);
            }
        }

        terminal.draw(|f| views::render(f, db, &state))?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => state.next_panel(),
                    KeyCode::Char('j') | KeyCode::Down => state.next_item(),
                    KeyCode::Char('k') | KeyCode::Up => state.prev_item(),
                    KeyCode::Char('r') => {
                        handle_retry(db, &mut state);
                    }
                    KeyCode::Char('s') => {
                        handle_skip(db, &mut state);
                    }
                    KeyCode::Char('R') => {
                        // Refresh: reload log file from scratch
                        state.log_lines = tailer.initial_load(LOG_TAIL_MAX_LINES);
                        state.set_status("Refreshed".to_string());
                    }
                    KeyCode::Char('?') => state.toggle_help(),
                    _ => {}
                }

                // Clear transient status messages on any keypress (except the one that set it)
                // We let the status stay for one render cycle
            }
        } else {
            // No event — clear status message after idle
            state.clear_status();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_retry(_db: &Database, state: &mut views::AppState) {
    state.set_status("Retry via GitHub: remove autodev:wip label".to_string());
}

fn handle_skip(_db: &Database, state: &mut views::AppState) {
    state.set_status("Skip via GitHub: add autodev:skip label".to_string());
}
