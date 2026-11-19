//! Where a board opened a session in herdr-app: a tab it is still opening, or
//! the terminal that tab reported. One per session, kept apart from the
//! session's own record.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub at: u64,
    /// `None` while the tab has not reported its terminal.
    pub terminal: Option<String>,
}

/// The entry a board saw before it acts: none, or the one written at `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    Nothing,
    At(u64),
}

impl Seen {
    pub fn parse(raw: &str) -> Result<Seen, String> {
        if raw == "none" {
            return Ok(Seen::Nothing);
        }
        raw.parse()
            .map(Seen::At)
            .map_err(|_| format!("--seen 은 none 또는 본 항목의 at 이에요 (받은 값: {raw})"))
    }

    fn matches(self, current: Option<&Opened>) -> bool {
        match (self, current) {
            (Seen::Nothing, None) => true,
            (Seen::At(at), Some(opened)) => opened.at == at,
            (Seen::Nothing, Some(_)) | (Seen::At(_), None) => false,
        }
    }
}

impl Opened {
    pub fn on(terminal: &str, now: u64) -> Opened {
        Opened {
            at: now,
            terminal: Some(terminal.to_string()),
        }
    }

    pub fn parse(raw: &str) -> Option<Opened> {
        let value: Value = serde_json::from_str(raw).ok()?;
        if value.get("v")?.as_u64()? != 1 {
            return None;
        }
        let terminal = match value.get("terminal")? {
            Value::Null => None,
            Value::String(t) => Some(t.clone()),
            _ => return None,
        };
        Some(Opened {
            at: value.get("at")?.as_u64()?,
            terminal,
        })
    }

    pub fn to_file(&self) -> String {
        json!({ "v": 1, "at": self.at, "terminal": self.terminal }).to_string()
    }

    pub fn to_json(&self) -> Value {
        json!({ "at": self.at, "terminal": self.terminal })
    }
}

/// A new tab is claimed only over the entry the board saw, so two boards that
/// saw the same entry open one tab between them.
pub fn claim(current: Option<&Opened>, seen: Seen, now: u64) -> Result<Opened, String> {
    if seen.matches(current) {
        Ok(Opened {
            at: now,
            terminal: None,
        })
    } else {
        Err(changed())
    }
}

/// A clear with `seen` removes only the entry the board saw.
pub fn may_clear(current: Option<&Opened>, seen: Option<Seen>) -> Result<(), String> {
    match seen {
        Some(seen) if !seen.matches(current) => Err(changed()),
        Some(_) | None => Ok(()),
    }
}

fn changed() -> String {
    "그 사이 다른 보드가 이 세션을 열었어요 — 보드를 다시 읽어 주세요".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening(at: u64) -> Opened {
        Opened { at, terminal: None }
    }

    #[test]
    fn a_claim_over_what_was_seen_opens_a_tab() {
        assert_eq!(claim(None, Seen::Nothing, 5), Ok(opening(5)));
        assert_eq!(
            claim(Some(&Opened::on("t1", 3)), Seen::At(3), 5),
            Ok(opening(5))
        );
    }

    #[test]
    fn a_claim_over_an_entry_written_since_is_refused() {
        assert!(claim(Some(&opening(4)), Seen::Nothing, 5).is_err());
        assert!(claim(Some(&Opened::on("t2", 4)), Seen::At(3), 5).is_err());
        assert!(claim(None, Seen::At(3), 5).is_err());
    }

    #[test]
    fn a_clear_keeps_an_entry_written_since() {
        assert!(may_clear(Some(&opening(4)), Some(Seen::At(4))).is_ok());
        assert!(may_clear(Some(&Opened::on("t2", 6)), Some(Seen::At(4))).is_err());
        assert!(may_clear(Some(&opening(4)), None).is_ok());
    }

    #[test]
    fn an_entry_reads_back_as_written() {
        for opened in [opening(7), Opened::on("t9", 8)] {
            assert_eq!(Opened::parse(&opened.to_file()), Some(opened));
        }
        assert_eq!(Opened::parse(r#"{"v":2,"at":1,"terminal":null}"#), None);
        assert_eq!(Opened::parse("garbage"), None);
    }

    #[test]
    fn seen_is_none_or_a_time() {
        assert_eq!(Seen::parse("none"), Ok(Seen::Nothing));
        assert_eq!(Seen::parse("42"), Ok(Seen::At(42)));
        assert!(Seen::parse("t1").is_err());
    }
}
