//! Shadow comparisons: the ledger replayed in order, each person's decision
//! set beside what the precedents before it would have answered, whichever
//! way the decision came in. Enough agreement recommends that a kind act.

use super::ledger::{Entry, EntryKind};
use super::precedent::{
    demoted, disabled, in_window, is_human, judge, mode, Book, DecisionKey, Mode,
};
use super::wait::Blocked;

pub const PROMOTE_AFTER: usize = 10;
const MAX_DISAGREED: usize = 1;
const RECENT: usize = 5;
pub const KINDS: [EntryKind; 3] = [
    EntryKind::Question,
    EntryKind::Permission,
    EntryKind::Decision,
];

/// One kind's comparisons in the window, and its mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindStats {
    pub kind: EntryKind,
    pub mode: Mode,
    pub compared: usize,
    pub agreed: usize,
    /// Of the newest comparisons, up to five.
    pub recent_agreed: usize,
    /// A person set it to act, and revokes stepped it back.
    pub demoted: bool,
}

impl KindStats {
    pub fn disagreed(&self) -> usize {
        self.compared - self.agreed
    }

    /// A recommendation only: a person decides.
    pub fn promotable(&self) -> bool {
        self.compared >= PROMOTE_AFTER
            && self.disagreed() <= MAX_DISAGREED
            && self.recent_agreed == RECENT
    }

    pub fn progress(&self) -> String {
        format!("{}/{PROMOTE_AFTER}", self.compared)
    }
}

/// A wait a precedent could have answered when it was answered, but for its
/// kind: a decision is never answered on precedents and still compared.
fn comparable(entry: &Entry) -> bool {
    let decision = Blocked::Decision.id();
    entry.key.is_some() && entry.blocked.iter().all(|b| *b == decision)
}

/// Whether the precedents before `entry` would have answered it, and alike.
fn compare(book: &Book, entry: &Entry) -> Option<bool> {
    let key = DecisionKey::parse(entry.key.as_deref()?).ok()?;
    let matched = judge(&book.before(entry), &key, entry.at);
    matched
        .eligible
        .then(|| matched.answer_fp.as_deref() == Some(entry.answer_fp.as_str()))
}

pub fn kind_stats(book: &Book, kind: EntryKind, now: u64) -> KindStats {
    let mut compared: Vec<(u64, &str, bool)> = book
        .entries()
        .iter()
        .filter(|e| e.kind == kind && is_human(e) && in_window(e, now) && comparable(e))
        .filter_map(|e| compare(book, e).map(|agreed| (e.at, e.id.as_str(), agreed)))
        .collect();
    compared.sort();
    let recent_agreed = compared
        .iter()
        .rev()
        .take(RECENT)
        .filter(|(_, _, agreed)| *agreed)
        .count();
    KindStats {
        kind,
        mode: mode(book, kind, now),
        compared: compared.len(),
        agreed: compared.iter().filter(|(_, _, agreed)| *agreed).count(),
        recent_agreed,
        demoted: demoted(book, kind, now),
    }
}

pub fn stats(book: &Book, now: u64) -> Vec<KindStats> {
    KINDS
        .iter()
        .map(|kind| kind_stats(book, *kind, now))
        .collect()
}

/// A key a person's revoke turned off and no direct decisions turned back on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledKey {
    pub key: String,
    pub kind: EntryKind,
    /// The proxy answer the newest revoke turned over, and when.
    pub entry: String,
    pub at: u64,
    pub reason: String,
}

pub fn disabled_keys(book: &Book, now: u64) -> Vec<DisabledKey> {
    let mut newest: Vec<&super::precedent::Revoke> = Vec::new();
    for revoke in book.revokes().iter().filter(|r| r.at <= now) {
        match newest.iter_mut().find(|r| r.key == revoke.key) {
            Some(kept) if (kept.at, &kept.id) < (revoke.at, &revoke.id) => *kept = revoke,
            Some(_) => {}
            None => newest.push(revoke),
        }
    }
    let mut keys: Vec<DisabledKey> = newest
        .into_iter()
        .filter(|r| disabled(book, &r.key, now))
        .map(|r| DisabledKey {
            key: r.key.clone(),
            kind: r.kind,
            entry: r.entry.clone(),
            at: r.at,
            reason: r.reason.clone(),
        })
        .collect();
    keys.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.key.cmp(&b.key)));
    keys
}
