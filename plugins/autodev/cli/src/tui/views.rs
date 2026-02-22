use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::queue::Database;

// ─── Panel enum ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Repos,
    ActiveItems,
    Labels,
    Logs,
}

// ─── Active queue item for display ───

#[derive(Debug, Clone)]
pub struct ActiveItem {
    pub id: String,
    pub queue_type: String,   // "issue" | "pr" | "merge"
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    pub status: String,
}

// ─── Label counts ───

#[derive(Debug, Clone, Default)]
pub struct LabelCounts {
    pub wip: i64,
    pub done: i64,
    pub skip: i64,
    pub failed: i64,
}

// ─── App state ───

pub struct AppState {
    pub active_panel: Panel,
    pub selected_index: usize,
    pub show_help: bool,
    pub log_lines: Vec<LogLine>,
    pub status_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub raw: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
    Trace,
    Unknown,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_panel: Panel::ActiveItems,
            selected_index: 0,
            show_help: false,
            log_lines: Vec::new(),
            status_message: None,
        }
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            Panel::Repos => Panel::ActiveItems,
            Panel::ActiveItems => Panel::Labels,
            Panel::Labels => Panel::Logs,
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

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }
}

// ─── Data queries ───

pub fn query_active_items(db: &Database) -> Vec<ActiveItem> {
    let conn = db.conn();
    let mut items = Vec::new();

    // Issues (non-terminal statuses)
    if let Ok(mut stmt) = conn.prepare(
        "SELECT iq.id, r.name, iq.github_number, iq.title, iq.status \
         FROM issue_queue iq JOIN repositories r ON iq.repo_id = r.id \
         WHERE iq.status NOT IN ('done', 'failed') \
         ORDER BY iq.updated_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(ActiveItem {
                id: row.get(0)?,
                queue_type: "issue".to_string(),
                repo_name: row.get(1)?,
                number: row.get(2)?,
                title: row.get(3)?,
                status: row.get(4)?,
            })
        }) {
            for row in rows.flatten() {
                items.push(row);
            }
        }
    }

    // PRs
    if let Ok(mut stmt) = conn.prepare(
        "SELECT pq.id, r.name, pq.github_number, pq.title, pq.status \
         FROM pr_queue pq JOIN repositories r ON pq.repo_id = r.id \
         WHERE pq.status NOT IN ('done', 'failed') \
         ORDER BY pq.updated_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(ActiveItem {
                id: row.get(0)?,
                queue_type: "pr".to_string(),
                repo_name: row.get(1)?,
                number: row.get(2)?,
                title: row.get(3)?,
                status: row.get(4)?,
            })
        }) {
            for row in rows.flatten() {
                items.push(row);
            }
        }
    }

    // Merges
    if let Ok(mut stmt) = conn.prepare(
        "SELECT mq.id, r.name, mq.pr_number, mq.title, mq.status \
         FROM merge_queue mq JOIN repositories r ON mq.repo_id = r.id \
         WHERE mq.status NOT IN ('done', 'failed') \
         ORDER BY mq.updated_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(ActiveItem {
                id: row.get(0)?,
                queue_type: "merge".to_string(),
                repo_name: row.get(1)?,
                number: row.get(2)?,
                title: row.get(3)?,
                status: row.get(4)?,
            })
        }) {
            for row in rows.flatten() {
                items.push(row);
            }
        }
    }

    items
}

pub fn query_label_counts(db: &Database) -> LabelCounts {
    let conn = db.conn();
    let mut counts = LabelCounts::default();

    // WIP = analyzing + processing + ready + reviewing + review_done + merging + conflict + pending
    let wip_statuses = "('pending','analyzing','processing','ready','reviewing','review_done','merging','conflict')";

    for table in &["issue_queue", "pr_queue", "merge_queue"] {
        let wip: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE status IN {wip_statuses}"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        counts.wip += wip;

        let done: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE status = 'done'"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        counts.done += done;

        let failed: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE status = 'failed'"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        counts.failed += failed;
    }

    // skip count from items marked with skip status (if present)
    // autodev:skip is a GitHub label, not a DB status; we approximate via 'failed' items
    // that were manually skipped. For now, skip = 0 as it's label-based.
    counts.skip = 0;

    counts
}

/// Get selected active item ID (for retry/skip actions)
pub fn selected_active_item<'a>(items: &'a [ActiveItem], state: &AppState) -> Option<&'a ActiveItem> {
    if state.active_panel == Panel::ActiveItems && state.selected_index < items.len() {
        Some(&items[state.selected_index])
    } else {
        None
    }
}

// ─── Rendering ───

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
    let home = crate::config::autodev_home(&crate::config::RealEnv);
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
            Constraint::Percentage(75), // detail area
        ])
        .split(area);

    render_repos_panel(f, chunks[0], db, state);

    // Right side: Active Items (35%) + Labels (20%) + Logs (45%)
    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35), // active items
            Constraint::Percentage(20), // labels
            Constraint::Percentage(45), // activity log
        ])
        .split(chunks[1]);

    render_active_items_panel(f, detail_chunks[0], db, state);
    render_labels_panel(f, detail_chunks[1], db, state);
    render_logs_panel(f, detail_chunks[2], state);

    // Render status message overlay if present
    if let Some(ref msg) = state.status_message {
        let msg_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(2),
            width: area.width.saturating_sub(4).min(msg.len() as u16 + 4),
            height: 1,
        };
        let status = Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(status, msg_area);
    }
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

fn render_active_items_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let active_items = query_active_items(db);

    let items: Vec<ListItem> = active_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let status_color = match item.status.as_str() {
                "pending" => Color::White,
                "analyzing" | "reviewing" | "merging" => Color::Cyan,
                "ready" | "review_done" => Color::Green,
                "processing" | "conflict" => Color::Yellow,
                _ => Color::DarkGray,
            };

            let selected = state.active_panel == Panel::ActiveItems && i == state.selected_index;
            let line_style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if selected { "▸" } else { " " };
            let type_icon = match item.queue_type.as_str() {
                "issue" => "I",
                "pr" => "P",
                "merge" => "M",
                _ => "?",
            };

            let title_max = 30;
            let title = if item.title.len() > title_max {
                format!("{}…", &item.title[..title_max])
            } else {
                item.title.clone()
            };

            ListItem::new(Line::from(vec![
                Span::raw(format!("{prefix} ")),
                Span::styled(
                    format!("[{type_icon}]"),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(format!(" {}#{} ", item.repo_name, item.number)),
                Span::styled(
                    format!("{:<12}", item.status),
                    Style::default().fg(status_color),
                ),
                Span::styled(title, Style::default().fg(Color::White)),
            ]))
            .style(line_style)
        })
        .collect();

    let count = active_items.len();
    let border_style = if state.active_panel == Panel::ActiveItems {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .title(format!(" Active Items ({count}) "))
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    f.render_widget(list, area);
}

fn render_labels_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let counts = query_label_counts(db);

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:wip  ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{:>4}", counts.wip),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(bar(counts.wip, 15)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:done ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{:>4}", counts.done),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(bar(counts.done, 15), Style::default().fg(Color::Green)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:skip ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:>4}", counts.skip),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(bar(counts.skip, 15), Style::default().fg(Color::Yellow)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("failed       ", Style::default().fg(Color::Red)),
            Span::styled(
                format!("{:>4}", counts.failed),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(bar(counts.failed, 15), Style::default().fg(Color::Red)),
        ])),
    ];

    let border_style = if state.active_panel == Panel::Labels {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" Labels ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    f.render_widget(list, area);
}

fn render_logs_panel(f: &mut Frame, area: Rect, state: &AppState) {
    let max_lines = area.height.saturating_sub(2) as usize; // minus border
    let total = state.log_lines.len();
    let start = total.saturating_sub(max_lines);
    let visible = &state.log_lines[start..];

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let global_idx = start + i;
            let selected = state.active_panel == Panel::Logs && global_idx == state.selected_index;

            let level_color = match line.level {
                LogLevel::Error => Color::Red,
                LogLevel::Warn => Color::Yellow,
                LogLevel::Info => Color::Green,
                LogLevel::Debug => Color::Cyan,
                LogLevel::Trace => Color::DarkGray,
                LogLevel::Unknown => Color::White,
            };

            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(level_color)
            };

            // Truncate long lines for display
            let display = if line.raw.len() > 120 {
                format!("{}…", &line.raw[..120])
            } else {
                line.raw.clone()
            };

            ListItem::new(format!("  {display}")).style(style)
        })
        .collect();

    let border_style = if state.active_panel == Panel::Logs {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    };

    let title = format!(" Activity Log ({total} lines) ");
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    f.render_widget(list, area);
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let text = if state.show_help {
        " Tab:panel  j/k:navigate  r:retry  s:skip  R:refresh  q:quit  ?:close help "
    } else {
        " Tab:panel  j/k:navigate  r:retry  s:skip  R:refresh  q:quit  ?:help "
    };

    let footer = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, area);
}

/// Generate a mini bar chart string
fn bar(count: i64, max_width: usize) -> String {
    let len = (count as usize).min(max_width);
    let filled = "█".repeat(len);
    let empty = "░".repeat(max_width.saturating_sub(len));
    format!("{filled}{empty}")
}
