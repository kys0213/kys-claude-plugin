//! Black-box port of git-utils `tests/core/jira.test.ts`
//! (`JiraService.detectTicket`).

use atelier::git::jira::{detect_ticket, JiraTicket};

fn ticket(raw: &str, normalized: &str) -> JiraTicket {
    JiraTicket {
        raw: raw.to_string(),
        normalized: normalized.to_string(),
    }
}

// ---------- detection succeeds ----------

#[test]
fn uppercase_direct_ticket() {
    assert_eq!(
        detect_ticket("WAD-0212"),
        Some(ticket("WAD-0212", "WAD-0212"))
    );
}

#[test]
fn prefix_uppercase() {
    assert_eq!(
        detect_ticket("feat/WAD-0212").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn prefix_lowercase_normalizes() {
    assert_eq!(
        detect_ticket("feat/wad-0212").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn fix_prefix() {
    assert_eq!(
        detect_ticket("fix/wad-2223").unwrap().normalized,
        "WAD-2223"
    );
}

#[test]
fn long_number() {
    assert_eq!(
        detect_ticket("PROJ-12345").unwrap().normalized,
        "PROJ-12345"
    );
}

#[test]
fn prefix_ticket_description() {
    assert_eq!(
        detect_ticket("feat/WAD-0212/add-login").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn hyphen_prefix() {
    assert_eq!(
        detect_ticket("feat-WAD-0212").unwrap().normalized,
        "WAD-0212"
    );
}

// ---------- detection returns None ----------

#[test]
fn plain_feature_branch_is_none() {
    assert_eq!(detect_ticket("feature/user-auth"), None);
}

#[test]
fn main_branch_is_none() {
    assert_eq!(detect_ticket("main"), None);
}

#[test]
fn digits_only_is_none() {
    assert_eq!(detect_ticket("12345"), None);
}

#[test]
fn empty_string_is_none() {
    assert_eq!(detect_ticket(""), None);
}

// ---------- edge cases ----------

#[test]
fn single_letter_key_is_none() {
    assert_eq!(detect_ticket("A-123"), None);
}

#[test]
fn trailing_chars_after_digits() {
    assert_eq!(
        detect_ticket("feat/WAD-0212abc").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn multiple_patterns_takes_first() {
    assert_eq!(
        detect_ticket("WAD-001-FIX-002").unwrap().normalized,
        "WAD-001"
    );
}
