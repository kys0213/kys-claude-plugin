//! Port of `git-utils/tests/core/jira.test.ts` — black-box detection cases.

use atelier::git::core::jira::detect_ticket;

#[test]
fn uppercase_direct_ticket() {
    let t = detect_ticket("WAD-0212").unwrap();
    assert_eq!(t.raw, "WAD-0212");
    assert_eq!(t.normalized, "WAD-0212");
}

#[test]
fn prefix_uppercase() {
    assert_eq!(
        detect_ticket("feat/WAD-0212").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn prefix_lowercase_normalized() {
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
fn long_number_ticket() {
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

#[test]
fn feature_branch_is_none() {
    assert!(detect_ticket("feature/user-auth").is_none());
}

#[test]
fn main_is_none() {
    assert!(detect_ticket("main").is_none());
}

#[test]
fn numbers_only_is_none() {
    assert!(detect_ticket("12345").is_none());
}

#[test]
fn empty_is_none() {
    assert!(detect_ticket("").is_none());
}

#[test]
fn single_char_key_is_none() {
    assert!(detect_ticket("A-123").is_none());
}

#[test]
fn trailing_chars_after_number() {
    assert_eq!(
        detect_ticket("feat/WAD-0212abc").unwrap().normalized,
        "WAD-0212"
    );
}

#[test]
fn first_match_wins() {
    assert_eq!(
        detect_ticket("WAD-001-FIX-002").unwrap().normalized,
        "WAD-001"
    );
}
