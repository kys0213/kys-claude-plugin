//! Decision keys: one key for the decisions that are the same decision. A key
//! is `<kind><rule version>:` and the first 128 bits of the SHA-256 of the
//! normalized parts, in hex.

use super::allowlist::{field, shell_line, Scope};
use super::question::Question;
use super::text::fold;
use sha2::{Digest, Sha256};
use std::path::Path;

const PART: char = '\u{1f}';
const ITEM: char = '\u{1e}';
const KEY_BYTES: usize = 16;

/// Control chars read as spaces, whitespace folded, ASCII lowercased, and the
/// trailing `? ？ . 。 !` taken off.
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

/// The first `bytes` bytes of the SHA-256 of the parts joined, in hex.
pub fn digest(parts: &[&str], bytes: usize) -> String {
    let joined = parts.join(&PART.to_string());
    Sha256::digest(joined.as_bytes())[..bytes]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn key(kind: &str, parts: &[&str]) -> String {
    format!("{kind}1:{}", digest(parts, KEY_BYTES))
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
    key("q", &[repo, &sentence(&question.question), &labels, mode])
}

pub fn decision_key(repo: &str, ask: &str, options: &[String]) -> String {
    let set = label_set(options.iter().map(String::as_str));
    key("d", &[repo, &sentence(ask), &set])
}

pub fn permission_key(repo: &str, mode: &str, tool: &str, target: &str) -> String {
    key("p", &[repo, mode, tool, target])
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
        assert_ne!(key("q", &["a"]), key("d", &["a"]));
        assert_eq!(key("q", &["a"]).len(), "q1:".len() + 32);
    }
}
