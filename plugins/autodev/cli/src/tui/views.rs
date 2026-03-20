use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::core::repository::RepoRepository;
use crate::infra::db::Database;

// ─── Panel enum ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Repos,
    ActiveItems,
    Labels,
    Logs,
}

// ─── View mode ───

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewMode {
    /// All repos overview (default)
    AllRepos,
    /// Per-repo detail view
    PerRepo,
}

// ─── Detail overlay ───

#[derive(Debug, Clone, PartialEq)]
pub enum DetailOverlay {
    /// Show detail for a specific active item
    ItemDetail(usize),
    /// Show HITL pending items
    Hitl,
    /// Show spec detail with acceptance criteria
    SpecDetail,
    /// Show Claw decision history
    ClawHistory,
}

// ─── Active queue item for display ───

#[derive(Debug, Clone)]
pub struct ActiveItem {
    #[allow(dead_code)]
    pub id: String,
    pub queue_type: crate::core::models::QueueType,
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
    pub view_mode: ViewMode,
    /// Index of the currently focused repo (used in per-repo view and for Enter navigation)
    pub focused_repo_index: usize,
    /// Cached repo names for navigation
    pub repo_names: Vec<String>,
    /// Detail overlay (shown on top of the current view)
    pub detail_overlay: Option<DetailOverlay>,
    pub repo_filter: Option<String>,
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
            view_mode: ViewMode::AllRepos,
            focused_repo_index: 0,
            repo_names: Vec::new(),
            detail_overlay: None,
            repo_filter: None,
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

    /// Switch between AllRepos and PerRepo views.
    /// In AllRepos: Tab switches to PerRepo for the selected repo.
    /// In PerRepo: Tab switches back to AllRepos.
    pub fn toggle_view_mode(&mut self) {
        match self.view_mode {
            ViewMode::AllRepos => {
                // Enter per-repo view for the currently selected repo
                if self.active_panel == Panel::Repos {
                    self.focused_repo_index = self.selected_index;
                }
                self.view_mode = ViewMode::PerRepo;
                self.selected_index = 0;
                self.detail_overlay = None;
            }
            ViewMode::PerRepo => {
                self.view_mode = ViewMode::AllRepos;
                self.selected_index = self.focused_repo_index;
                self.active_panel = Panel::Repos;
                self.detail_overlay = None;
            }
        }
    }

    /// Enter key: in AllRepos with Repos panel, go to per-repo detail.
    /// In ActiveItems panel, show item detail overlay.
    pub fn enter_selected(&mut self) {
        match self.view_mode {
            ViewMode::AllRepos => {
                if self.active_panel == Panel::Repos {
                    self.focused_repo_index = self.selected_index;
                    self.view_mode = ViewMode::PerRepo;
                    self.selected_index = 0;
                } else if self.active_panel == Panel::ActiveItems {
                    self.detail_overlay = Some(DetailOverlay::ItemDetail(self.selected_index));
                }
            }
            ViewMode::PerRepo => {
                if self.active_panel == Panel::ActiveItems {
                    self.detail_overlay = Some(DetailOverlay::ItemDetail(self.selected_index));
                }
            }
        }
    }

    /// Navigate to next repo in per-repo view (Right arrow).
    pub fn next_repo(&mut self) {
        if self.view_mode == ViewMode::PerRepo && !self.repo_names.is_empty() {
            self.focused_repo_index = (self.focused_repo_index + 1) % self.repo_names.len();
            self.selected_index = 0;
            self.detail_overlay = None;
        }
    }

    /// Navigate to previous repo in per-repo view (Left arrow).
    pub fn prev_repo(&mut self) {
        if self.view_mode == ViewMode::PerRepo && !self.repo_names.is_empty() {
            if self.focused_repo_index == 0 {
                self.focused_repo_index = self.repo_names.len() - 1;
            } else {
                self.focused_repo_index -= 1;
            }
            self.selected_index = 0;
            self.detail_overlay = None;
        }
    }

    /// Show HITL overlay.
    pub fn show_hitl(&mut self) {
        self.detail_overlay = Some(DetailOverlay::Hitl);
    }

    /// Show spec detail overlay.
    pub fn show_spec_detail(&mut self) {
        self.detail_overlay = Some(DetailOverlay::SpecDetail);
    }

    /// Show Claw decision history overlay.
    pub fn show_claw_history(&mut self) {
        self.detail_overlay = Some(DetailOverlay::ClawHistory);
    }

    /// Dismiss any detail overlay.
    pub fn dismiss_overlay(&mut self) {
        self.detail_overlay = None;
    }

    /// Handle 'q' key: in per-repo view go back to all-repos, in all-repos quit (returns true).
    pub fn handle_quit(&mut self) -> bool {
        if self.detail_overlay.is_some() {
            self.detail_overlay = None;
            return false;
        }
        match self.view_mode {
            ViewMode::PerRepo => {
                self.view_mode = ViewMode::AllRepos;
                self.selected_index = self.focused_repo_index;
                self.active_panel = Panel::Repos;
                false
            }
            ViewMode::AllRepos => true,
        }
    }

    /// Update cached repo names from DB.
    pub fn refresh_repo_names(&mut self, db: &Database) {
        self.repo_names = db
            .repo_status_summary()
            .map(|rows| rows.into_iter().map(|r| r.name).collect())
            .unwrap_or_default();
    }
}

// ─── Data queries ───

pub fn query_active_items(status_path: &std::path::Path) -> Vec<ActiveItem> {
    let status = match crate::service::daemon::status::read_status(status_path) {
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
    match crate::service::daemon::status::read_status(status_path) {
        Some(s) => LabelCounts {
            wip: s.wip,
            done: 0,
            skip: 0,
            failed: 0,
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

    render_header(f, chunks[0], db, state);
    render_body(f, chunks[1], db, status_path, state);
    render_footer(f, chunks[2], state);
}

fn render_header(f: &mut Frame, area: Rect, db: &Database, state: &AppState) {
    let home = crate::core::config::autodev_home(&crate::core::config::RealEnv);
    let running = crate::service::daemon::pid::is_running(&home);
    let status = if running {
        Span::styled("● running", Style::default().fg(Color::Green))
    } else {
        Span::styled("○ stopped", Style::default().fg(Color::Red))
    };

    let repo_count = db.repo_status_summary().map(|v| v.len()).unwrap_or(0);

    let view_indicator = match state.view_mode {
        ViewMode::AllRepos => Span::styled(
            " [All Repos] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        ViewMode::PerRepo => {
            let repo_name = state
                .repo_names
                .get(state.focused_repo_index)
                .map(|s| s.as_str())
                .unwrap_or("?");
            Span::styled(
                format!(" [Repo: {repo_name}] "),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )
        }
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " autodev v0.1.0 ",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        status,
        Span::raw(format!("  │  {repo_count} repos  │  ")),
        view_indicator,
        Span::raw("  │  [?] help"),
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
    match state.view_mode {
        ViewMode::AllRepos => render_body_all_repos(f, area, db, status_path, state),
        ViewMode::PerRepo => render_body_per_repo(f, area, db, status_path, state),
    }

    // Render detail overlay on top if present
    if let Some(ref overlay) = state.detail_overlay {
        render_detail_overlay(f, area, db, status_path, state, overlay);
    }

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

fn render_body_all_repos(
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
}

fn render_body_per_repo(
    f: &mut Frame,
    area: Rect,
    db: &Database,
    status_path: &std::path::Path,
    state: &AppState,
) {
    let repo_name = state
        .repo_names
        .get(state.focused_repo_index)
        .cloned()
        .unwrap_or_default();

    let home = crate::core::config::autodev_home(&crate::core::config::RealEnv);
    let board_state = crate::tui::board::BoardStateBuilder::build(db, Some(&repo_name), &home).ok();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // board / specs
            Constraint::Percentage(30), // active items (filtered)
            Constraint::Percentage(30), // logs
        ])
        .split(area);

    // Top: Spec + kanban summary for this repo
    render_repo_board_panel(f, chunks[0], &repo_name, board_state.as_ref(), state);

    // Middle: Active items filtered to this repo
    render_active_items_filtered(f, chunks[1], status_path, &repo_name, state);

    // Bottom: Logs
    render_logs_panel(f, chunks[2], state);
}

fn render_repo_board_panel(
    f: &mut Frame,
    area: Rect,
    repo_name: &str,
    board_state: Option<&crate::core::board::BoardState>,
    state: &AppState,
) {
    let mut lines: Vec<Line> = Vec::new();

    if let Some(bs) = board_state {
        if let Some(repo) = bs.repos.iter().find(|r| r.repo_name == repo_name) {
            // Specs
            for (i, spec) in repo.specs.iter().enumerate() {
                let selected = state.active_panel == Panel::Repos && i == state.selected_index;
                let prefix = if selected { "▸ " } else { "  " };
                let hitl_str = if spec.hitl_count > 0 {
                    format!("  HITL: {}", spec.hitl_count)
                } else {
                    String::new()
                };
                let style = if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "{prefix}{} [{}] {}{hitl_str}",
                        spec.title, spec.status, spec.progress
                    ),
                    style,
                )));
                if let Some(ref ac) = spec.acceptance_criteria {
                    for ac_line in ac.lines().filter(|l| !l.trim().is_empty()) {
                        lines.push(Line::from(Span::styled(
                            format!("    {ac_line}"),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }

            // Kanban summary
            if !repo.columns.is_empty() {
                lines.push(Line::from(""));
                let col_summary: Vec<String> = repo
                    .columns
                    .iter()
                    .map(|c| format!("{}: {}", c.name, c.items.len()))
                    .collect();
                lines.push(Line::from(Span::styled(
                    format!("  Kanban: {}", col_summary.join(" | ")),
                    Style::default().fg(Color::Cyan),
                )));
            }

            // Orphan issues
            if !repo.orphan_issues.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  Unlinked issues: {}", repo.orphan_issues.len()),
                    Style::default().fg(Color::Yellow),
                )));
            }
        } else {
            lines.push(Line::from(Span::raw("  No data for this repo")));
        }
    } else {
        lines.push(Line::from(Span::raw("  Loading...")));
    }

    let nav_hint = if state.repo_names.len() > 1 {
        format!(
            " ({}/{}) ←/→ switch repo ",
            state.focused_repo_index + 1,
            state.repo_names.len()
        )
    } else {
        String::new()
    };

    let block = Block::default()
        .title(format!(" {repo_name}{nav_hint}"))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, area);
}

fn render_active_items_filtered(
    f: &mut Frame,
    area: Rect,
    status_path: &std::path::Path,
    repo_name: &str,
    state: &AppState,
) {
    let all_items = query_active_items(status_path);
    let filtered: Vec<&ActiveItem> = all_items
        .iter()
        .filter(|item| item.repo_name == repo_name)
        .collect();

    let items: Vec<ListItem> = filtered
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

            let title = truncate_str(&item.title, 40);

            ListItem::new(Line::from(vec![
                Span::raw(format!("{prefix} ")),
                Span::styled(
                    format!("[{type_icon}]"),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(format!(" #{} ", item.number)),
                Span::styled(
                    format!("{:<12}", item.status),
                    Style::default().fg(status_color),
                ),
                Span::styled(title, Style::default().fg(Color::White)),
            ]))
            .style(line_style)
        })
        .collect();

    let count = filtered.len();
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

fn render_detail_overlay(
    f: &mut Frame,
    area: Rect,
    db: &Database,
    status_path: &std::path::Path,
    state: &AppState,
    overlay: &DetailOverlay,
) {
    // Center overlay in the body area
    let overlay_area = centered_rect(70, 60, area);

    let (title, lines) = match overlay {
        DetailOverlay::ItemDetail(idx) => {
            let items = query_active_items(status_path);
            if let Some(item) = items.get(*idx) {
                (
                    format!(" {} #{} ", item.repo_name, item.number),
                    vec![
                        Line::from(Span::styled(
                            item.title.clone(),
                            Style::default().add_modifier(Modifier::BOLD),
                        )),
                        Line::from(""),
                        Line::from(format!("  Type:   {}", item.queue_type.as_str())),
                        Line::from(format!("  Status: {}", item.status)),
                        Line::from(format!("  Repo:   {}", item.repo_name)),
                        Line::from(""),
                        Line::from(Span::styled(
                            "  Press q/Esc to close",
                            Style::default().fg(Color::DarkGray),
                        )),
                    ],
                )
            } else {
                (" Detail ".to_string(), vec![Line::from("  Item not found")])
            }
        }
        DetailOverlay::Hitl => {
            let home = crate::core::config::autodev_home(&crate::core::config::RealEnv);
            let board = crate::tui::board::BoardStateBuilder::build(db, None, &home).ok();
            let pending = board.as_ref().map(|b| b.hitl_summary.pending).unwrap_or(0);
            let total = board.as_ref().map(|b| b.hitl_summary.total).unwrap_or(0);
            (
                " HITL Items ".to_string(),
                vec![
                    Line::from(format!("  Pending: {pending}")),
                    Line::from(format!("  Total:   {total}")),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  Use 'autodev hitl respond' in another terminal to act.",
                        Style::default().fg(Color::DarkGray),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "  Press q/Esc to close",
                        Style::default().fg(Color::DarkGray),
                    )),
                ],
            )
        }
        DetailOverlay::SpecDetail => {
            let repo_filter = if state.view_mode == ViewMode::PerRepo {
                state
                    .repo_names
                    .get(state.focused_repo_index)
                    .map(|s| s.as_str())
            } else {
                None
            };
            let home = crate::core::config::autodev_home(&crate::core::config::RealEnv);
            let board = crate::tui::board::BoardStateBuilder::build(db, repo_filter, &home).ok();
            let mut detail_lines = Vec::new();
            if let Some(bs) = board.as_ref() {
                for repo in &bs.repos {
                    detail_lines.push(Line::from(Span::styled(
                        format!("  {}", repo.repo_name),
                        Style::default().add_modifier(Modifier::BOLD),
                    )));
                    for spec in &repo.specs {
                        detail_lines.push(Line::from(format!(
                            "    {} [{}] {}",
                            spec.title, spec.status, spec.progress
                        )));
                        if let Some(ref ac) = spec.acceptance_criteria {
                            for ac_line in ac.lines().filter(|l| !l.trim().is_empty()) {
                                detail_lines.push(Line::from(Span::styled(
                                    format!("      {ac_line}"),
                                    Style::default().fg(Color::DarkGray),
                                )));
                            }
                        }
                    }
                    detail_lines.push(Line::from(""));
                }
            }
            if detail_lines.is_empty() {
                detail_lines.push(Line::from("  No specs found"));
            }
            detail_lines.push(Line::from(Span::styled(
                "  Press q/Esc to close",
                Style::default().fg(Color::DarkGray),
            )));
            (" Spec Detail ".to_string(), detail_lines)
        }
        DetailOverlay::ClawHistory => (
            " Claw Decision History ".to_string(),
            vec![
                Line::from("  Decision history is shown in the Activity Log."),
                Line::from("  Filter log lines containing 'claw' or 'decision'."),
                Line::from(""),
                Line::from(Span::styled(
                    "  Press q/Esc to close",
                    Style::default().fg(Color::DarkGray),
                )),
            ],
        ),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(lines).block(block);
    f.render_widget(paragraph, overlay_area);
}

/// Create a centered Rect using percentages of the parent area.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
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
    let all_items = query_active_items(status_path);
    let active_items: Vec<&ActiveItem> = if let Some(ref filter) = state.repo_filter {
        all_items
            .iter()
            .filter(|i| i.repo_name == *filter)
            .collect()
    } else {
        all_items.iter().collect()
    };

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
    let text = if state.detail_overlay.is_some() {
        " q/Esc:close overlay "
    } else if state.show_help {
        match state.view_mode {
            ViewMode::AllRepos => {
                " Tab:per-repo  j/k:navigate  Enter:detail  h:hitl  s:specs  d:claw  r:retry  R:refresh  q:quit  ?:close "
            }
            ViewMode::PerRepo => {
                " Tab:all-repos  ←/→:switch repo  j/k:navigate  Enter:detail  h:hitl  s:specs  d:claw  q:back  ?:close "
            }
        }
    } else {
        match state.view_mode {
            ViewMode::AllRepos => {
                " Tab:per-repo  j/k:nav  Enter:select  h:hitl  s:specs  d:claw  q:quit  ?:help "
            }
            ViewMode::PerRepo => {
                " Tab:all-repos  ←/→:repo  j/k:nav  Enter:select  h:hitl  s:specs  q:back  ?:help "
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with_repos(names: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.repo_names = names.iter().map(|s| s.to_string()).collect();
        state
    }

    #[test]
    fn toggle_view_mode_switches_between_all_and_per_repo() {
        let mut state = state_with_repos(&["org/a", "org/b"]);
        assert_eq!(state.view_mode, ViewMode::AllRepos);

        state.active_panel = Panel::Repos;
        state.selected_index = 1;
        state.toggle_view_mode();
        assert_eq!(state.view_mode, ViewMode::PerRepo);
        assert_eq!(state.focused_repo_index, 1);
        assert_eq!(state.selected_index, 0);

        state.toggle_view_mode();
        assert_eq!(state.view_mode, ViewMode::AllRepos);
        assert_eq!(state.selected_index, 1);
        assert_eq!(state.active_panel, Panel::Repos);
    }

    #[test]
    fn enter_selected_in_repos_panel_switches_to_per_repo() {
        let mut state = state_with_repos(&["org/a", "org/b"]);
        state.active_panel = Panel::Repos;
        state.selected_index = 0;

        state.enter_selected();
        assert_eq!(state.view_mode, ViewMode::PerRepo);
        assert_eq!(state.focused_repo_index, 0);
    }

    #[test]
    fn enter_selected_in_active_items_opens_overlay() {
        let mut state = state_with_repos(&["org/a"]);
        state.active_panel = Panel::ActiveItems;
        state.selected_index = 2;

        state.enter_selected();
        assert_eq!(state.detail_overlay, Some(DetailOverlay::ItemDetail(2)));
    }

    #[test]
    fn next_prev_repo_wraps_around() {
        let mut state = state_with_repos(&["org/a", "org/b", "org/c"]);
        state.view_mode = ViewMode::PerRepo;
        state.focused_repo_index = 0;

        state.next_repo();
        assert_eq!(state.focused_repo_index, 1);

        state.next_repo();
        assert_eq!(state.focused_repo_index, 2);

        state.next_repo();
        assert_eq!(state.focused_repo_index, 0); // wrap

        state.prev_repo();
        assert_eq!(state.focused_repo_index, 2); // wrap back
    }

    #[test]
    fn next_prev_repo_noop_in_all_repos_mode() {
        let mut state = state_with_repos(&["org/a", "org/b"]);
        state.view_mode = ViewMode::AllRepos;
        state.focused_repo_index = 0;

        state.next_repo();
        assert_eq!(state.focused_repo_index, 0); // no change

        state.prev_repo();
        assert_eq!(state.focused_repo_index, 0); // no change
    }

    #[test]
    fn handle_quit_in_per_repo_goes_back() {
        let mut state = state_with_repos(&["org/a"]);
        state.view_mode = ViewMode::PerRepo;
        state.focused_repo_index = 0;

        let should_exit = state.handle_quit();
        assert!(!should_exit);
        assert_eq!(state.view_mode, ViewMode::AllRepos);
    }

    #[test]
    fn handle_quit_in_all_repos_exits() {
        let mut state = state_with_repos(&["org/a"]);
        state.view_mode = ViewMode::AllRepos;

        let should_exit = state.handle_quit();
        assert!(should_exit);
    }

    #[test]
    fn handle_quit_dismisses_overlay_first() {
        let mut state = state_with_repos(&["org/a"]);
        state.view_mode = ViewMode::AllRepos;
        state.detail_overlay = Some(DetailOverlay::Hitl);

        let should_exit = state.handle_quit();
        assert!(!should_exit);
        assert!(state.detail_overlay.is_none());

        // Now quit should exit
        let should_exit = state.handle_quit();
        assert!(should_exit);
    }

    #[test]
    fn shortcut_keys_set_overlays() {
        let mut state = AppState::new();

        state.show_hitl();
        assert_eq!(state.detail_overlay, Some(DetailOverlay::Hitl));

        state.show_spec_detail();
        assert_eq!(state.detail_overlay, Some(DetailOverlay::SpecDetail));

        state.show_claw_history();
        assert_eq!(state.detail_overlay, Some(DetailOverlay::ClawHistory));

        state.dismiss_overlay();
        assert!(state.detail_overlay.is_none());
    }

    #[test]
    fn toggle_view_mode_clears_overlay() {
        let mut state = state_with_repos(&["org/a"]);
        state.detail_overlay = Some(DetailOverlay::Hitl);

        state.toggle_view_mode();
        assert!(state.detail_overlay.is_none());
    }
}
