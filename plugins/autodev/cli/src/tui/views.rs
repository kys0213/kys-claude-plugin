use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::domain::repository::RepoRepository;
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
    #[allow(dead_code)]
    pub id: String,
    pub queue_type: String, // "issue" | "pr" | "merge"
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

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_panel: Panel::ActiveItems,
            selected_index: 0,
            show_help: false,
            log_lines: Vec::new(),
            status_message: None,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
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

pub fn query_active_items(status_path: &std::path::Path) -> Vec<ActiveItem> {
    let status = match crate::daemon::status::read_status(status_path) {
        Some(s) => s,
        None => return Vec::new(),
    };
    status
        .active_items
        .into_iter()
        .map(|si| ActiveItem {
            id: si.work_id.clone(),
            queue_type: si.queue_type,
            repo_name: si.repo_name,
            number: si.number,
            title: si.title,
            status: si.phase,
        })
        .collect()
}

pub fn query_label_counts(status_path: &std::path::Path) -> LabelCounts {
    match crate::daemon::status::read_status(status_path) {
        Some(s) => LabelCounts {
            wip: s.counters.wip,
            done: s.counters.done,
            skip: s.counters.skip,
            failed: s.counters.failed,
        },
        None => LabelCounts::default(),
    }
}

// ─── Rendering ───

pub fn render(f: &mut Frame, db: &Database, status_path: &std::path::Path, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // body
            Constraint::Length(1), // footer
        ])
        .split(f.area());

    render_header(f, chunks[0], db);
    render_body(f, chunks[1], db, status_path, state);
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

    let repo_count = db.repo_status_summary().map(|v| v.len()).unwrap_or(0);

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

fn render_body(
    f: &mut Frame,
    area: Rect,
    db: &Database,
    status_path: &std::path::Path,
    state: &AppState,
) {
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

    render_active_items_panel(f, detail_chunks[0], status_path, state);
    render_labels_panel(f, detail_chunks[1], status_path, state);
    render_logs_panel(f, detail_chunks[2], state);

    // Render status message overlay if present
    if let Some(ref msg) = state.status_message {
        let msg_area = Rect {
            x: area.x + 2,
            y: area.y + area.height.saturating_sub(2),
            width: area.width.saturating_sub(4).min(msg.len() as u16 + 4),
            height: 1,
        };
        let status = Paragraph::new(msg.as_str()).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        f.render_widget(status, msg_area);
    }
}

fn render_repos_panel(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let repos: Vec<(String, bool)> = db
        .repo_status_summary()
        .map(|rows| rows.into_iter().map(|r| (r.name, r.enabled)).collect())
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

fn render_active_items_panel(
    f: &mut Frame,
    area: Rect,
    status_path: &std::path::Path,
    state: &AppState,
) {
    let active_items = query_active_items(status_path);

    let items: Vec<ListItem> = active_items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let status_color = match item.status.as_str() {
                "Pending" => Color::White,
                "Analyzing" | "Reviewing" | "Merging" => Color::Cyan,
                "Ready" | "ReviewDone" => Color::Green,
                "Implementing" | "Improving" | "Conflict" => Color::Yellow,
                "Improved" => Color::Blue,
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

            let title = truncate_str(&item.title, 30);

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

fn render_labels_panel(f: &mut Frame, area: Rect, status_path: &std::path::Path, state: &AppState) {
    let counts = query_label_counts(status_path);
    let max_count = [counts.wip, counts.done, counts.skip, counts.failed]
        .into_iter()
        .max()
        .unwrap_or(0);

    let items = vec![
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:wip  ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{:>4}", counts.wip),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(bar_scaled(counts.wip, max_count, 15)),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:done ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{:>4}", counts.done),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                bar_scaled(counts.done, max_count, 15),
                Style::default().fg(Color::Green),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("autodev:skip ", Style::default().fg(Color::Yellow)),
            Span::styled(
                format!("{:>4}", counts.skip),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                bar_scaled(counts.skip, max_count, 15),
                Style::default().fg(Color::Yellow),
            ),
        ])),
        ListItem::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("failed       ", Style::default().fg(Color::Red)),
            Span::styled(
                format!("{:>4}", counts.failed),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                bar_scaled(counts.failed, max_count, 15),
                Style::default().fg(Color::Red),
            ),
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
            let display = truncate_str(&line.raw, 120);

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

fn bar_scaled(count: i64, max_count: i64, max_width: usize) -> String {
    let len = if max_count > 0 {
        ((count as f64 / max_count as f64) * max_width as f64).ceil() as usize
    } else {
        0
    };
    let len = len.min(max_width);
    let filled = "█".repeat(len);
    let empty = "░".repeat(max_width.saturating_sub(len));
    format!("{filled}{empty}")
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}
