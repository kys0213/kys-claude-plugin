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
use crate::queue::repository::QueueAdmin;
use crate::queue::Database;

use events::LogTailer;
use views::{query_active_items, Panel};

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

fn handle_retry(db: &Database, state: &mut views::AppState) {
    if state.active_panel != Panel::ActiveItems {
        state.set_status("Select an item in Active Items panel first".to_string());
        return;
    }

    let items = query_active_items(db);
    if let Some(item) = views::selected_active_item(&items, state) {
        // Only failed items can be retried; active items are still in-progress
        // Check if the item is in 'failed' state by querying directly
        let id = item.id.clone();
        match db.queue_retry(&id) {
            Ok(true) => {
                state.set_status(format!("Retried: {}#{}", item.repo_name, item.number));
            }
            Ok(false) => {
                state.set_status("Item is not in failed state".to_string());
            }
            Err(e) => {
                state.set_status(format!("Retry error: {e}"));
            }
        }
    } else {
        state.set_status("No item selected".to_string());
    }
}

fn handle_skip(db: &Database, state: &mut views::AppState) {
    if state.active_panel != Panel::ActiveItems {
        state.set_status("Select an item in Active Items panel first".to_string());
        return;
    }

    let items = query_active_items(db);
    if let Some(item) = views::selected_active_item(&items, state) {
        // Mark item as done (skip) — remove from active processing
        let conn = db.conn();
        let table = match item.queue_type.as_str() {
            "issue" => "issue_queue",
            "pr" => "pr_queue",
            "merge" => "merge_queue",
            _ => {
                state.set_status("Unknown queue type".to_string());
                return;
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        match conn.execute(
            &format!(
                "UPDATE {table} SET status = 'done', error_message = 'skipped via dashboard', \
                 updated_at = ?2 WHERE id = ?1 AND status NOT IN ('done')"
            ),
            rusqlite::params![item.id, now],
        ) {
            Ok(n) if n > 0 => {
                state.set_status(format!("Skipped: {}#{}", item.repo_name, item.number));
            }
            Ok(_) => {
                state.set_status("Item already completed".to_string());
            }
            Err(e) => {
                state.set_status(format!("Skip error: {e}"));
            }
        }
    } else {
        state.set_status("No item selected".to_string());
    }
}
