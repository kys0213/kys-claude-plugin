use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::queue::Database;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Repos,
    Queues,
    Logs,
}

pub struct AppState {
    pub active_panel: Panel,
    pub selected_index: usize,
    pub show_help: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_panel: Panel::Repos,
            selected_index: 0,
            show_help: false,
        }
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Repos => Panel::Queues,
            Panel::Queues => Panel::Logs,
            Panel::Logs => Panel::Repos,
        };
        self.selected_index = 0;
    }

    pub fn next_item(&mut self) {
        self.selected_index = self.selected_index.saturating_add(1);
    }

    pub fn prev_item(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

pub fn render(f: &mut Frame, db: &Database, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),   // body
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    render_header(f, chunks[0], db);
    render_body(f, chunks[1], db, state);
    render_footer(f, chunks[2], state);
}

fn render_header(f: &mut Frame, area: Rect, db: &Database) {
    let home = crate::config::autodev_home();
    let running = crate::daemon::pid::is_running(&home);
    let status = if running {
        Span::styled("● running", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ stopped", Style::default().fg(Color::Red))
    };

    let repo_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM repositories", [], |row| row.get(0))
        .unwrap_or(0);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " autodev v0.1.0 ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        status,
        Span::raw(format!("  │  {repo_count} repos  │  [?] help")),
    ]))
    .block(Block::default().borders(Borders::ALL));

    f.render_widget(header, area);
}

fn render_body(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // repos
            Constraint::Percentage(75), // detail
        ])
        .split(area);

    render_repos_panel(f, chunks[0], db, state);

    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // queues
            Constraint::Percentage(60), // logs
        ])
        .split(chunks[1]);

    render_queues_panel(f, detail_chunks[0], db, state);
    render_logs_panel(f, detail_chunks[1], db);
}

fn render_repos_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let conn = db.conn();
    let repos: Vec<(String, bool)> = conn
        .prepare("SELECT name, enabled FROM repositories ORDER BY name")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let items: Vec<ListItem> = repos
        .iter()
        .enumerate()
        .map(|(i, (name, enabled))| {
            let icon = if *enabled { "●" } else { "○" };
            let style = if state.active_panel == Panel::Repos && i == state.selected_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!(" {icon} {name}")).style(style)
        })
        .collect();

    let border_style = if state.active_panel == Panel::Repos {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" Repos ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    f.render_widget(list, area);
}

fn render_queues_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let conn = db.conn();

    let counts: Vec<(String, i64)> = ["issue_queue", "pr_queue", "merge_queue"]
        .iter()
        .map(|table| {
            let label = table.replace("_queue", "");
            let count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE status NOT IN ('done', 'failed')"),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            (label, count)
        })
        .collect();

    let items: Vec<ListItem> = counts
        .iter()
        .map(|(label, count)| {
            let bar_len = (*count as usize).min(20);
            let bar = "█".repeat(bar_len) + &"░".repeat(20 - bar_len);
            ListItem::new(format!("  {label:<8} {bar}  {count} active"))
        })
        .collect();

    let border_style = if state.active_panel == Panel::Queues {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" Queues ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    f.render_widget(list, area);
}

fn render_logs_panel(f: &mut Frame, area: Rect, db: &Database) {
    let conn = db.conn();

    let logs: Vec<String> = conn
        .prepare(
            "SELECT started_at, queue_type, command, exit_code \
             FROM consumer_logs ORDER BY started_at DESC LIMIT 15",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let time: String = row.get(0)?;
                let qtype: String = row.get(1)?;
                let cmd: String = row.get(2)?;
                let exit: Option<i32> = row.get(3)?;
                let icon = match exit {
                    Some(0) => "✓",
                    Some(_) => "✗",
                    None => "…",
                };
                // 시간 축약
                let short_time = time.get(11..19).unwrap_or(&time);
                let short_cmd = if cmd.len() > 50 {
                    format!("{}…", &cmd[..50])
                } else {
                    cmd
                };
                Ok(format!("  {short_time} [{qtype}] {icon} {short_cmd}"))
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let items: Vec<ListItem> = logs.iter().map(|l| ListItem::new(l.as_str())).collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Activity Log ")
            .borders(Borders::ALL),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let text = if state.show_help {
        " Tab:panel  j/k:navigate  r:retry  q:quit  ?:close help "
    } else {
        " Tab:panel  j/k:navigate  r:retry  q:quit  ?:help "
    };

    let footer = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, area);
}
