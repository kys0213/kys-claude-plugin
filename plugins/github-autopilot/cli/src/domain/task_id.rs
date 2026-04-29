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

    pub fn as_str(&self) -> &str {
        &self.0
    }
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
}
