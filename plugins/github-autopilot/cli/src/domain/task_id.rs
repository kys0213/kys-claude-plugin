use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new_deterministic(epic: &str, section_path: &str, requirement: &str) -> Self {
        let canon = format!(
            "{}::{}::{}",
            epic,
            normalize_section(section_path),
            slug(requirement)
        );
        let digest = Sha256::digest(canon.as_bytes());
        Self(hex::encode(digest)[..12].to_string())
    }

    pub fn from_raw(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// Validates that `raw` is shaped like a deterministic task id —
    /// 12 lowercase hex characters — and returns a `TaskId`. Returns
    /// `Err(TaskIdParseError)` with an actionable message otherwise.
    ///
    /// Use this on every CLI input boundary that mutates state (e.g.,
    /// `task add <task_id>`, `task add-batch`) so a typo can't silently
    /// insert a row whose id will never match the deterministic form.
    /// Read-only paths (`task show`, `task release`, ...) may keep
    /// using [`Self::from_raw`] — a missing-id lookup already surfaces
    /// the typo without corrupting state.
    pub fn parse(raw: &str) -> Result<Self, TaskIdParseError> {
        if raw.len() != TASK_ID_HEX_LEN {
            return Err(TaskIdParseError::InvalidLength {
                got: raw.to_string(),
                len: raw.len(),
            });
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'))
        {
            return Err(TaskIdParseError::InvalidChars {
                got: raw.to_string(),
            });
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Length of a deterministic task id (lowercase hex prefix of SHA-256).
pub const TASK_ID_HEX_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskIdParseError {
    #[error(
        "invalid task id '{got}': expected 12 lowercase hex characters (e.g. 'a1b2c3d4e5f6'), got {len} characters"
    )]
    InvalidLength { got: String, len: usize },

    #[error("invalid task id '{got}': must contain only lowercase hex characters [0-9a-f]")]
    InvalidChars { got: String },
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn normalize_section(p: &str) -> String {
    let nfkc: String = p.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let trimmed = lower.trim();
    let mut out = String::with_capacity(trimmed.len());
    let mut last_was_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    out
}

fn slug(s: &str) -> String {
    let nfkc: String = s.nfkc().collect();
    let lower = nfkc.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_was_dash = true;
    for ch in lower.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_is_stable_across_runs() {
        let a = TaskId::new_deterministic("e", "## 인증", "토큰 갱신");
        let b = TaskId::new_deterministic("e", "## 인증", "토큰 갱신");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 12);
    }

    #[test]
    fn task_id_normalizes_whitespace_in_section() {
        let a = TaskId::new_deterministic("e", "## 인증", "토큰 갱신");
        let b = TaskId::new_deterministic("e", "##  인증  ", "토큰  갱신");
        assert_eq!(a, b);
    }

    #[test]
    fn task_id_normalizes_nfkc() {
        let a = TaskId::new_deterministic("e", "## 인증", "토큰 갱신");
        let b = TaskId::new_deterministic("e", "## 인증", "토큰\u{00A0}갱신");
        assert_eq!(a, b);
    }

    #[test]
    fn task_id_differs_on_meaningful_change() {
        let a = TaskId::new_deterministic("e", "## 인증", "토큰 갱신");
        let b = TaskId::new_deterministic("e", "## 인증", "토큰 발급");
        assert_ne!(a, b);
    }

    #[test]
    fn task_id_differs_across_epics() {
        let a = TaskId::new_deterministic("e1", "## 인증", "토큰 갱신");
        let b = TaskId::new_deterministic("e2", "## 인증", "토큰 갱신");
        assert_ne!(a, b);
    }

    #[test]
    fn slug_collapses_multiple_separators() {
        assert_eq!(slug("hello   world!!!"), "hello-world");
        assert_eq!(slug("---abc---"), "abc");
        assert_eq!(slug("foo/bar.baz"), "foo-bar-baz");
    }

    #[test]
    fn parse_accepts_canonical_form() {
        let id = TaskId::parse("a1b2c3d4e5f6").unwrap();
        assert_eq!(id.as_str(), "a1b2c3d4e5f6");
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let err = TaskId::parse("abc").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("12 lowercase hex"), "msg: {msg}");
        assert!(msg.contains("got 3 characters"), "msg: {msg}");
    }

    #[test]
    fn parse_rejects_uppercase() {
        let err = TaskId::parse("A1B2C3D4E5F6").unwrap_err();
        assert!(err.to_string().contains("lowercase hex"));
    }

    #[test]
    fn parse_rejects_non_hex_chars() {
        let err = TaskId::parse("g1b2c3d4e5f6").unwrap_err();
        assert!(err.to_string().contains("lowercase hex"));
    }

    #[test]
    fn parse_rejects_typo() {
        // The kind of input the task description warned about: an 11-char
        // id (typo from copy/paste) should be rejected with an actionable
        // length message rather than silently accepted by the store.
        let err = TaskId::parse("a1b2c3d4e5f").unwrap_err();
        assert!(err.to_string().contains("got 11 characters"));
    }

    #[test]
    fn parse_accepts_deterministic_output() {
        // The deterministic generator's output must round-trip through parse —
        // that's the contract that makes parse() a safe gate on user input.
        let id = TaskId::new_deterministic("e", "## section", "requirement");
        TaskId::parse(id.as_str()).expect("deterministic id must parse");
    }
}
