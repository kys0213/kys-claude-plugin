//! Decision keys: one key for the decisions that are the same decision. A key
//! is `<kind><rule version>:` and the first 128 bits of the SHA-256 of the
//! normalized parts, in hex.

use super::allowlist::{field, shell_line, Scope};
use super::ledger::EntryKind;
use super::question::Question;
use super::text::fold;
use sha2::{Digest, Sha256};
use std::path::Path;

const PART: char = '\u{1f}';
const ITEM: char = '\u{1e}';
const KEY_BYTES: usize = 16;
const RULE_VERSION: u32 = 1;
const KEY_KINDS: [EntryKind; 3] = [
    EntryKind::Question,
    EntryKind::Permission,
    EntryKind::Decision,
];

pub const KEY_HEX_LEN: usize = KEY_BYTES * 2;

pub fn sentence(raw: &str) -> String {
    let spaced: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    fold(&spaced)
        .to_ascii_lowercase()
        .trim_end_matches(['?', '？', '.', '。', '!', ' '])
        .to_string()
}

pub fn digest(parts: &[&str], bytes: usize) -> String {
    let joined = parts.join(&PART.to_string());
    Sha256::digest(joined.as_bytes())[..bytes]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn key_prefix(kind: EntryKind) -> String {
    let tag = match kind {
        EntryKind::Question => 'q',
        EntryKind::Permission => 'p',
        EntryKind::Decision => 'd',
    };
    format!("{tag}{RULE_VERSION}")
}

pub fn kind_of_prefix(prefix: &str) -> Option<EntryKind> {
    KEY_KINDS
        .into_iter()
        .find(|kind| key_prefix(*kind) == prefix)
}

/// Every prefix a key may start with, as `q1:`.
pub fn key_prefixes() -> Vec<String> {
    KEY_KINDS
        .into_iter()
        .map(|kind| format!("{}:", key_prefix(kind)))
        .collect()
}

fn key(kind: EntryKind, parts: &[&str]) -> String {
    format!("{}:{}", key_prefix(kind), digest(parts, KEY_BYTES))
}

/// One answer's fingerprint: the same for answers that read the same as a
/// `sentence`, whatever the kind of wait.
pub fn answer_fp(answer: &str) -> String {
    digest(&[&sentence(answer)], KEY_BYTES)
}

fn label_set<'a>(labels: impl Iterator<Item = &'a str>) -> String {
    let mut set: Vec<String> = labels.map(sentence).collect();
    set.sort();
    set.dedup();
    set.join(&ITEM.to_string())
}

pub fn question_key(repo: &str, question: &Question) -> String {
    let labels = label_set(question.options.iter().map(|o| o.label.as_str()));
    let mode = if question.multi_select {
        "multi"
    } else {
        "single"
    };
    key(
        EntryKind::Question,
        &[repo, &sentence(&question.question), &labels, mode],
    )
}

pub fn decision_key(repo: &str, ask: &str, options: &[String]) -> String {
    let set = label_set(options.iter().map(String::as_str));
    key(EntryKind::Decision, &[repo, &sentence(ask), &set])
}

pub fn permission_key(repo: &str, mode: &str, tool: &str, target: &str) -> String {
    key(EntryKind::Permission, &[repo, mode, tool, target])
}

fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => host.split(':').next()?,
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn parent_dir(path: &str) -> String {
    match Path::new(path).parent() {
        Some(parent) => parent.to_string_lossy().into_owned(),
        None => String::new(),
    }
}

/// What a permission request's key groups by. `None` for a shell line that is
/// not one simple command or a path outside the repository.
pub fn permission_target(tool: &str, input: &[(String, String)], scope: &Scope) -> Option<String> {
    match tool {
        "Bash" => {
            let line = shell_line(field(input, "command")?);
            if !line.is_simple() {
                return None;
            }
            Some(
                line.commands[0]
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        }
        "Read" | "Edit" | "Write" => scope.relative(&parent_dir(field(input, "file_path")?)),
        "NotebookEdit" => scope.relative(&parent_dir(field(input, "notebook_path")?)),
        "Glob" | "Grep" => scope.relative(field(input, "path").unwrap_or("")),
        "WebFetch" => host(field(input, "url")?),
        other => Some(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_folds_space_case_and_the_closing_mark() {
        assert_eq!(sentence("  Which\tCOLOR ?? "), "which color");
        assert_eq!(sentence("배포할까요？"), "배포할까요");
        assert_eq!(sentence("a\u{1f}b"), "a b");
    }

    #[test]
    fn a_host_drops_the_scheme_user_port_and_path() {
        assert_eq!(
            host("https://u:p@Example.COM:8443/a?b"),
            Some("example.com".to_string())
        );
        assert_eq!(host("http://[::1]:80/"), Some("::1".to_string()));
        assert_eq!(host("https:///x"), None);
    }

    #[test]
    fn keys_differ_by_kind() {
        assert_ne!(
            key(EntryKind::Question, &["a"]),
            key(EntryKind::Decision, &["a"])
        );
        assert_eq!(key(EntryKind::Question, &["a"]).len(), "q1:".len() + 32);
    }
}
