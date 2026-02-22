use std::path::Path;

use autodev::queue::Database;
use autodev::tui::views;

fn open_memory_db() -> Database {
    let db = Database::open(Path::new(":memory:")).expect("open in-memory db");
    db.initialize().expect("initialize schema");
    db
}

// ─── query_active_items ───

#[test]
fn test_active_items_returns_empty() {
    let db = open_memory_db();
    let items = views::query_active_items(&db);
    assert!(items.is_empty(), "query_active_items should always return empty vec");
}

// ─── query_label_counts ───

#[test]
fn test_label_counts_returns_zeros() {
    let db = open_memory_db();
    let counts = views::query_label_counts(&db);
    assert_eq!(counts.wip, 0);
    assert_eq!(counts.done, 0);
    assert_eq!(counts.skip, 0);
    assert_eq!(counts.failed, 0);
}

// ─── Panel navigation ───

#[test]
fn test_panel_navigation() {
    let mut state = views::AppState::new();
    assert_eq!(state.active_panel, views::Panel::ActiveItems);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Labels);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Logs);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::Repos);

    state.next_panel();
    assert_eq!(state.active_panel, views::Panel::ActiveItems);
}

// ─── selected_active_item ───

#[test]
fn test_selected_active_item_returns_none_when_empty() {
    let items: Vec<views::ActiveItem> = Vec::new();
    let state = views::AppState::new();
    let selected = views::selected_active_item(&items, &state);
    assert!(selected.is_none(), "should return None when no active items exist");
}
