use std::collections::HashSet;

use anyhow::Result;

use crate::core::board::*;
use crate::core::models::QueuePhase;
use crate::core::repository::*;
use crate::infra::db::Database;

// ─── BoardStateBuilder ───

/// Builds a `BoardState` from database queries.
pub struct BoardStateBuilder;

const COLUMN_NAMES: &[&str] = &["Pending", "Ready", "Running", "Done", "Skipped"];

impl BoardStateBuilder {
    /// Build board state from DB, optionally filtered by repo name.
    pub fn build(
        db: &Database,
        repo_filter: Option<&str>,
        home: &std::path::Path,
    ) -> Result<BoardState> {
        let all_repos = db.workspace_find_enabled()?;
        let all_items = db.queue_list_items(repo_filter)?;
        let all_specs = db.spec_list(repo_filter)?;
        let all_spec_issues = db.spec_issues_all()?;

        let mut board_repos = Vec::new();

        let repos: Vec<_> = if let Some(filter) = repo_filter {
            all_repos.into_iter().filter(|r| r.name == filter).collect()
        } else {
            all_repos
        };

        for repo in &repos {
            let repo_items: Vec<_> = all_items.iter().filter(|i| i.repo_id == repo.id).collect();
            let repo_specs: Vec<_> = all_specs.iter().filter(|s| s.repo_id == repo.id).collect();

            // Skip repos with no items and no specs
            if repo_items.is_empty() && repo_specs.is_empty() {
                continue;
            }

            // Pre-compute: set of done work_ids for this repo
            let done_work_ids: HashSet<&str> = repo_items
                .iter()
                .filter(|qi| qi.phase == QueuePhase::Done)
                .map(|qi| qi.work_id.as_str())
                .collect();

            // Fetch HITL items for this repo
            let repo_hitl_events = db.hitl_list(Some(&repo.name)).unwrap_or_default();
            let hitl_count = repo_hitl_events
                .iter()
                .filter(|e| e.status == crate::core::models::HitlStatus::Pending)
                .count() as u32;

            // Build spec entries
            let mut spec_entries = Vec::new();
            for spec in &repo_specs {
                let empty_vec = Vec::new();
                let linked_issues = all_spec_issues.get(&spec.id).unwrap_or(&empty_vec);
                let total = linked_issues.len();
                let done_count = linked_issues
                    .iter()
                    .filter(|si| {
                        // work_id format is "issue:<repo_name>:<number>"
                        let work_id = format!("issue:{}:{}", repo.name, si.issue_number);
                        done_work_ids.contains(work_id.as_str())
                    })
                    .count();

                // Collect HITL items linked to this spec
                let hitl_items: Vec<HitlBoardItem> = repo_hitl_events
                    .iter()
                    .filter(|e| e.spec_id.as_deref() == Some(&spec.id))
                    .map(|e| HitlBoardItem {
                        severity: e.severity.to_string(),
                        situation: e.situation.clone(),
                        options: e.options.clone(),
                        status: e.status.to_string(),
                    })
                    .collect();

                spec_entries.push(SpecBoardEntry {
                    id: spec.id.clone(),
                    title: spec.title.clone(),
                    status: spec.status.to_string(),
                    progress: format!("{done_count}/{total}"),
                    hitl_count,
                    hitl_items,
                    acceptance_criteria: spec.acceptance_criteria.clone(),
                });
            }

            // Build columns
            let columns: Vec<BoardColumn> = COLUMN_NAMES
                .iter()
                .map(|col_name| {
                    let phase = col_name.to_lowercase();
                    let items: Vec<BoardItem> = repo_items
                        .iter()
                        .filter(|qi| qi.phase.as_str() == phase)
                        .map(|qi| BoardItem {
                            work_id: qi.work_id.clone(),
                            title: qi.title.clone().unwrap_or_default(),
                            queue_type: qi.queue_type.to_string(),
                        })
                        .collect();
                    BoardColumn {
                        name: col_name.to_string(),
                        items,
                    }
                })
                .collect();

            // Orphan issues: queue items not linked to any spec
            let linked_issue_numbers: std::collections::HashSet<i64> = repo_specs
                .iter()
                .flat_map(|s| all_spec_issues.get(&s.id).cloned().unwrap_or_default())
                .map(|si| si.issue_number)
                .collect();
            let orphan_issues: Vec<BoardItem> = repo_items
                .iter()
                .filter(|qi| qi.queue_type == crate::core::models::QueueType::Issue)
                .filter(|qi| {
                    // Extract issue number from work_id and check if not linked
                    !qi.work_id
                        .rsplit(':')
                        .next()
                        .and_then(|n| n.parse::<i64>().ok())
                        .is_some_and(|n| linked_issue_numbers.contains(&n))
                })
                .map(|qi| BoardItem {
                    work_id: qi.work_id.clone(),
                    title: qi.title.clone().unwrap_or_default(),
                    queue_type: qi.queue_type.to_string(),
                })
                .collect();

            board_repos.push(RepoBoardState {
                repo_name: repo.name.clone(),
                specs: spec_entries,
                columns,
                orphan_issues,
            });
        }

        // Cross-repo HITL summary
        let hitl_pending = db.hitl_pending_count(None)?;
        let hitl_total = db.hitl_total_count(None)?;

        // Claw daemon status (check if status file exists)
        let claw_running = home.join("daemon.status.json").exists();

        // Recent claw decisions (Fix 5)
        let decisions = db.decision_list(None, 10).unwrap_or_default();
        let recent_decisions: Vec<DecisionBoardItem> = decisions
            .into_iter()
            .map(|d| DecisionBoardItem {
                decision_type: d.decision_type.as_str().to_string(),
                target_work_id: d.target_work_id,
                reasoning: d.reasoning,
                created_at: d.created_at,
            })
            .collect();

        Ok(BoardState {
            repos: board_repos,
            hitl_summary: HitlSummary {
                pending: hitl_pending,
                total: hitl_total,
            },
            claw_status: ClawStatus {
                running: claw_running,
                last_decision_at: None,
                tick_interval_secs: None,
            },
            recent_decisions,
        })
    }
}

// ─── TextBoardRenderer ───

/// Simple text-based renderer for `autodev board` CLI output.
pub struct TextBoardRenderer;

impl BoardRenderer for TextBoardRenderer {
    fn render(&self, state: &BoardState) -> String {
        if state.repos.is_empty() {
            return "No board data available.\n".to_string();
        }

        let mut out = String::new();

        // Header: Claw status + HITL summary
        let claw_icon = if state.claw_status.running {
            "●"
        } else {
            "○"
        };
        out.push_str(&format!("Claw: {claw_icon}"));
        if state.hitl_summary.pending > 0 {
            out.push_str(&format!(
                "  HITL: {} pending / {} total",
                state.hitl_summary.pending, state.hitl_summary.total
            ));
        }
        out.push_str("\n\n");

        for (i, repo) in state.repos.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&repo.repo_name);
            out.push('\n');

            // Specs
            for spec in &repo.specs {
                out.push_str(&format!(
                    "  Specs: {} [{}] {}",
                    spec.title,
                    capitalize_first(&spec.status),
                    spec.progress
                ));
                if spec.hitl_count > 0 {
                    out.push_str(&format!("  HITL: {}", spec.hitl_count));
                }
                out.push('\n');

                // Fix 4: Acceptance criteria with check status
                if let Some(ref ac) = spec.acceptance_criteria {
                    for line in ac.lines().filter(|l| !l.trim().is_empty()) {
                        let trimmed = line.trim();
                        if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
                            let desc = trimmed
                                .trim_start_matches("- [x]")
                                .trim_start_matches("- [X]")
                                .trim();
                            out.push_str(&format!("    [PASS] {desc}\n"));
                        } else if trimmed.starts_with("- [ ]") {
                            let desc = trimmed.trim_start_matches("- [ ]").trim();
                            out.push_str(&format!("    [FAIL] {desc}\n"));
                        } else {
                            out.push_str(&format!("    {line}\n"));
                        }
                    }
                }

                // Fix 3: HITL items detail
                if !spec.hitl_items.is_empty() {
                    out.push_str("    HITL Items:\n");
                    for item in &spec.hitl_items {
                        out.push_str(&format!(
                            "      [{severity}] {situation} ({status})\n",
                            severity = item.severity,
                            situation = item.situation,
                            status = item.status,
                        ));
                        if !item.options.is_empty() {
                            out.push_str(&format!("        options: {}\n", item.options));
                        }
                    }
                }
            }

            // Kanban table
            out.push_str(&render_kanban_table(&repo.columns));
        }

        // Fix 5: Recent claw decisions
        if !state.recent_decisions.is_empty() {
            out.push_str("\nClaw Decisions:\n");
            for d in &state.recent_decisions {
                let target = d.target_work_id.as_deref().unwrap_or("-");
                out.push_str(&format!(
                    "  [{type}] {target} — {reason} ({at})\n",
                    type = d.decision_type,
                    reason = d.reasoning,
                    at = d.created_at,
                ));
            }
        }

        out
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn render_kanban_table(columns: &[BoardColumn]) -> String {
    if columns.is_empty() {
        return String::new();
    }

    // Calculate column widths (minimum 9 chars for header + padding)
    let col_widths: Vec<usize> = columns
        .iter()
        .map(|col| {
            let header_w = col.name.len();
            let max_item_w = col
                .items
                .iter()
                .map(|item| format_item_id(&item.work_id).len())
                .max()
                .unwrap_or(0);
            header_w.max(max_item_w).max(5)
        })
        .collect();

    let max_rows = columns.iter().map(|c| c.items.len()).max().unwrap_or(0);

    let mut out = String::new();

    // Top border
    out.push_str("  ");
    out.push_str(&horizontal_line(&col_widths, '┌', '┬', '┐'));
    out.push('\n');

    // Header row
    out.push_str("  │");
    for (col, w) in columns.iter().zip(col_widths.iter()) {
        out.push_str(&format!(" {:^width$} │", col.name, width = w));
    }
    out.push('\n');

    // Header separator
    out.push_str("  ");
    out.push_str(&horizontal_line(&col_widths, '├', '┼', '┤'));
    out.push('\n');

    // Data rows
    if max_rows == 0 {
        // Single empty row
        out.push_str("  │");
        for w in &col_widths {
            out.push_str(&format!(" {:width$} │", "", width = w));
        }
        out.push('\n');
    } else {
        for row in 0..max_rows {
            out.push_str("  │");
            for (col, w) in columns.iter().zip(col_widths.iter()) {
                let cell = col
                    .items
                    .get(row)
                    .map(|item| format_item_id(&item.work_id))
                    .unwrap_or_default();
                out.push_str(&format!(" {:<width$} │", cell, width = w));
            }
            out.push('\n');
        }
    }

    // Bottom border
    out.push_str("  ");
    out.push_str(&horizontal_line(&col_widths, '└', '┴', '┘'));
    out.push('\n');

    out
}

/// Extract short item ID from work_id (e.g. "org/repo#42" → "#42")
fn format_item_id(work_id: &str) -> String {
    if let Some(pos) = work_id.rfind('#') {
        work_id[pos..].to_string()
    } else {
        work_id.to_string()
    }
}

fn horizontal_line(widths: &[usize], left: char, mid: char, right: char) -> String {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        for _ in 0..(w + 2) {
            s.push('─');
        }
        if i < widths.len() - 1 {
            s.push(mid);
        }
    }
    s.push(right);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── TextBoardRenderer tests ───

    #[test]
    fn text_renderer_empty_state() {
        let renderer = TextBoardRenderer;
        let state = BoardState::default();
        let output = renderer.render(&state);
        assert_eq!(output, "No board data available.\n");
    }

    #[test]
    fn text_renderer_formats_columns_correctly() {
        let renderer = TextBoardRenderer;
        let state = BoardState {
            repos: vec![RepoBoardState {
                repo_name: "org/repo-a".to_string(),
                specs: vec![SpecBoardEntry {
                    id: "s1".to_string(),
                    title: "Auth Module v2".to_string(),
                    status: "active".to_string(),
                    progress: "3/5".to_string(),
                    hitl_count: 1,
                    hitl_items: vec![],
                    acceptance_criteria: None,
                }],
                columns: vec![
                    BoardColumn {
                        name: "Pending".to_string(),
                        items: vec![BoardItem {
                            work_id: "org/repo-a#46".to_string(),
                            title: "Fix login".to_string(),
                            queue_type: "issue".to_string(),
                        }],
                    },
                    BoardColumn {
                        name: "Ready".to_string(),
                        items: vec![BoardItem {
                            work_id: "org/repo-a#45".to_string(),
                            title: "Add signup".to_string(),
                            queue_type: "issue".to_string(),
                        }],
                    },
                    BoardColumn {
                        name: "Running".to_string(),
                        items: vec![BoardItem {
                            work_id: "org/repo-a#44".to_string(),
                            title: "Refactor auth".to_string(),
                            queue_type: "issue".to_string(),
                        }],
                    },
                    BoardColumn {
                        name: "Done".to_string(),
                        items: vec![
                            BoardItem {
                                work_id: "org/repo-a#42".to_string(),
                                title: "Setup DB".to_string(),
                                queue_type: "issue".to_string(),
                            },
                            BoardItem {
                                work_id: "org/repo-a#43".to_string(),
                                title: "Add tests".to_string(),
                                queue_type: "issue".to_string(),
                            },
                        ],
                    },
                    BoardColumn {
                        name: "Skipped".to_string(),
                        items: vec![],
                    },
                ],
                orphan_issues: vec![],
            }],
            ..Default::default()
        };

        let output = renderer.render(&state);

        // Verify structure
        assert!(output.contains("org/repo-a"));
        assert!(output.contains("Auth Module v2"));
        assert!(output.contains("[Active]"));
        assert!(output.contains("3/5"));
        assert!(output.contains("HITL: 1"));
        assert!(output.contains("#46"));
        assert!(output.contains("#45"));
        assert!(output.contains("#44"));
        assert!(output.contains("#42"));
        assert!(output.contains("#43"));
        // Table borders
        assert!(output.contains('┌'));
        assert!(output.contains('┘'));
        assert!(output.contains("Pending"));
        assert!(output.contains("Done"));
    }

    #[test]
    fn text_renderer_handles_empty_columns() {
        let renderer = TextBoardRenderer;
        let state = BoardState {
            repos: vec![RepoBoardState {
                repo_name: "org/empty".to_string(),
                specs: vec![],
                columns: vec![
                    BoardColumn {
                        name: "Pending".to_string(),
                        items: vec![],
                    },
                    BoardColumn {
                        name: "Done".to_string(),
                        items: vec![],
                    },
                ],
                orphan_issues: vec![],
            }],
            ..Default::default()
        };

        let output = renderer.render(&state);
        assert!(output.contains("org/empty"));
        // Should have table with empty cells
        assert!(output.contains("Pending"));
        assert!(output.contains("Done"));
    }

    #[test]
    fn text_renderer_handles_multiple_repos() {
        let renderer = TextBoardRenderer;
        let state = BoardState {
            repos: vec![
                RepoBoardState {
                    repo_name: "org/repo-a".to_string(),
                    specs: vec![],
                    columns: vec![BoardColumn {
                        name: "Pending".to_string(),
                        items: vec![BoardItem {
                            work_id: "org/repo-a#1".to_string(),
                            title: "Task A".to_string(),
                            queue_type: "issue".to_string(),
                        }],
                    }],
                    orphan_issues: vec![],
                },
                RepoBoardState {
                    repo_name: "org/repo-b".to_string(),
                    specs: vec![],
                    columns: vec![BoardColumn {
                        name: "Done".to_string(),
                        items: vec![BoardItem {
                            work_id: "org/repo-b#2".to_string(),
                            title: "Task B".to_string(),
                            queue_type: "pr".to_string(),
                        }],
                    }],
                    orphan_issues: vec![],
                },
            ],
            ..Default::default()
        };

        let output = renderer.render(&state);
        assert!(output.contains("org/repo-a"));
        assert!(output.contains("org/repo-b"));
        assert!(output.contains("#1"));
        assert!(output.contains("#2"));
    }

    #[test]
    fn text_renderer_no_hitl_when_zero() {
        let renderer = TextBoardRenderer;
        let state = BoardState {
            repos: vec![RepoBoardState {
                repo_name: "org/repo".to_string(),
                specs: vec![SpecBoardEntry {
                    id: "s1".to_string(),
                    title: "Feature X".to_string(),
                    status: "active".to_string(),
                    progress: "0/3".to_string(),
                    hitl_count: 0,
                    hitl_items: vec![],
                    acceptance_criteria: None,
                }],
                columns: vec![],
                orphan_issues: vec![],
            }],
            ..Default::default()
        };

        let output = renderer.render(&state);
        assert!(output.contains("Feature X"));
        assert!(!output.contains("HITL"));
    }

    #[test]
    fn format_item_id_extracts_hash() {
        assert_eq!(format_item_id("org/repo#42"), "#42");
        assert_eq!(format_item_id("no-hash"), "no-hash");
        assert_eq!(format_item_id("#1"), "#1");
    }

    // ─── BoardStateBuilder integration tests (with real DB) ───

    fn setup_test_db(dir: &std::path::Path) -> Database {
        let db_path = dir.join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn builder_empty_db_returns_empty_state() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());

        let state = BoardStateBuilder::build(&db, None, tmp.path()).unwrap();
        assert!(state.repos.is_empty());
    }

    #[test]
    fn builder_groups_items_by_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());

        // Add two repos
        let repo_a_id = db
            .workspace_add("https://github.com/org/repo-a", "org/repo-a")
            .unwrap();
        let repo_b_id = db
            .workspace_add("https://github.com/org/repo-b", "org/repo-b")
            .unwrap();

        // Add queue items
        insert_queue_item(
            &db,
            "org/repo-a#1",
            &repo_a_id,
            "issue",
            "pending",
            Some("Task A1"),
        );
        insert_queue_item(
            &db,
            "org/repo-a#2",
            &repo_a_id,
            "issue",
            "done",
            Some("Task A2"),
        );
        insert_queue_item(
            &db,
            "org/repo-b#10",
            &repo_b_id,
            "pr",
            "running",
            Some("PR B1"),
        );

        let state = BoardStateBuilder::build(&db, None, tmp.path()).unwrap();
        assert_eq!(state.repos.len(), 2);

        let repo_a = state
            .repos
            .iter()
            .find(|r| r.repo_name == "org/repo-a")
            .unwrap();
        let repo_b = state
            .repos
            .iter()
            .find(|r| r.repo_name == "org/repo-b")
            .unwrap();

        // repo-a should have items in Pending and Done
        let pending = repo_a.columns.iter().find(|c| c.name == "Pending").unwrap();
        assert_eq!(pending.items.len(), 1);
        assert_eq!(pending.items[0].work_id, "org/repo-a#1");

        let done = repo_a.columns.iter().find(|c| c.name == "Done").unwrap();
        assert_eq!(done.items.len(), 1);

        // repo-b should have item in Running
        let running = repo_b.columns.iter().find(|c| c.name == "Running").unwrap();
        assert_eq!(running.items.len(), 1);
        assert_eq!(running.items[0].queue_type, "pr");
    }

    #[test]
    fn builder_includes_spec_progress() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());

        let repo_id = db
            .workspace_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        // Add a spec
        use crate::core::models::NewSpec;
        let spec_id = db
            .spec_add(&NewSpec {
                repo_id: repo_id.clone(),
                title: "Auth Module".to_string(),
                body: "Implement auth".to_string(),
                source_path: None,
                test_commands: None,
                acceptance_criteria: None,
            })
            .unwrap();

        // Link issues to spec
        db.spec_link_issue(&spec_id, 1).unwrap();
        db.spec_link_issue(&spec_id, 2).unwrap();
        db.spec_link_issue(&spec_id, 3).unwrap();

        // Add queue items — #1 is done, #2 is running, #3 is pending
        // work_id format: "issue:<repo_name>:<number>"
        insert_queue_item(
            &db,
            "issue:org/repo:1",
            &repo_id,
            "issue",
            "done",
            Some("Task 1"),
        );
        insert_queue_item(
            &db,
            "issue:org/repo:2",
            &repo_id,
            "issue",
            "running",
            Some("Task 2"),
        );
        insert_queue_item(
            &db,
            "issue:org/repo:3",
            &repo_id,
            "issue",
            "pending",
            Some("Task 3"),
        );

        let state = BoardStateBuilder::build(&db, None, tmp.path()).unwrap();
        assert_eq!(state.repos.len(), 1);

        let repo = &state.repos[0];
        assert_eq!(repo.specs.len(), 1);
        assert_eq!(repo.specs[0].title, "Auth Module");
        assert_eq!(repo.specs[0].progress, "1/3"); // 1 done out of 3 linked
    }

    #[test]
    fn builder_filters_by_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());

        let repo_a_id = db
            .workspace_add("https://github.com/org/repo-a", "org/repo-a")
            .unwrap();
        let repo_b_id = db
            .workspace_add("https://github.com/org/repo-b", "org/repo-b")
            .unwrap();

        insert_queue_item(
            &db,
            "org/repo-a#1",
            &repo_a_id,
            "issue",
            "pending",
            Some("A"),
        );
        insert_queue_item(
            &db,
            "org/repo-b#1",
            &repo_b_id,
            "issue",
            "pending",
            Some("B"),
        );

        let state = BoardStateBuilder::build(&db, Some("org/repo-a"), tmp.path()).unwrap();
        assert_eq!(state.repos.len(), 1);
        assert_eq!(state.repos[0].repo_name, "org/repo-a");
    }

    /// Helper to insert a queue item directly via SQL.
    fn insert_queue_item(
        db: &Database,
        work_id: &str,
        repo_id: &str,
        queue_type: &str,
        phase: &str,
        title: Option<&str>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        db.conn()
            .execute(
                "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![work_id, repo_id, queue_type, phase, title, now],
            )
            .unwrap();
    }
}
