//! Record copies on disk: `<config>/atelier/kanban/records/<session>.json`,
//! one per session, written by each session's mod and read here so a terminal
//! sees the board without the pane.

use crate::kanban::commands::inbox::{config_dir, InboxStore};
use crate::kanban::commands::payload::parse_record_copy;
use crate::kanban::core::board::{build, Board, BoardParams, Column};
use crate::kanban::core::request::check_id;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const SUFFIX: &str = ".json";
const STALE_AFTER_MS: u64 = 2 * 60 * 1_000;
const EXPIRE_AFTER_MS: u64 = 24 * 60 * 60 * 1_000;

pub trait RecordStore {
    fn path(&self, session: &str) -> Result<PathBuf, String>;
    /// Every copy's contents in file-name order; refused when the directory
    /// is not there.
    fn read_all(&self) -> Result<Vec<String>, String>;
    /// Succeeds when there is no copy.
    fn remove(&self, session: &str) -> Result<(), String>;
}

pub struct FsRecords {
    root: Result<PathBuf, String>,
}

impl FsRecords {
    pub fn from_env() -> FsRecords {
        FsRecords {
            root: config_dir().map(|c| c.join("atelier").join("kanban").join("records")),
        }
    }
}

impl RecordStore for FsRecords {
    fn path(&self, session: &str) -> Result<PathBuf, String> {
        Ok(self.root.clone()?.join(format!("{session}{SUFFIX}")))
    }

    fn read_all(&self) -> Result<Vec<String>, String> {
        let dir = self.root.clone()?;
        let entries = fs::read_dir(&dir).map_err(|e| {
            format!(
                "기록 사본 디렉터리를 읽지 못했어요 ({}): {e}",
                dir.display()
            )
        })?;
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|name| !name.starts_with('.') && name.ends_with(SUFFIX))
            .collect();
        names.sort();
        Ok(names
            .iter()
            .filter_map(|name| fs::read_to_string(dir.join(name)).ok())
            .collect())
    }

    fn remove(&self, session: &str) -> Result<(), String> {
        match fs::remove_file(self.path(session)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("기록 사본을 지우지 못했어요: {e}")),
        }
    }
}

pub struct StatusQuery {
    pub columns: Vec<String>,
    pub session: Option<String>,
    pub now: u64,
}

pub struct KanbanRecords<'a> {
    pub store: &'a dyn RecordStore,
    pub inbox: &'a dyn InboxStore,
}

fn answer(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|e| json!({ "error": e }))
}

impl KanbanRecords<'_> {
    pub fn path(&self, session: &str) -> Value {
        answer((|| {
            check_id("session", session)?;
            let path = self.store.path(session)?;
            Ok(json!({ "path": path.to_string_lossy() }))
        })())
    }

    pub fn forget(&self, session: &str) -> Value {
        answer((|| {
            check_id("session", session)?;
            self.store.remove(session)?;
            self.inbox.clear(session)?;
            Ok(json!({ "forgot": session }))
        })())
    }

    /// The board of the readable copies, narrowed to the chosen columns.
    pub fn status(&self, query: &StatusQuery) -> Result<Board, String> {
        let columns = Column::parse_all(&query.columns)?;
        if let Some(session) = &query.session {
            check_id("session", session)?;
        }
        let sessions: Vec<_> = self
            .store
            .read_all()?
            .iter()
            .filter_map(|raw| parse_record_copy(raw))
            .collect();
        let params = BoardParams {
            now: query.now,
            stale_after: STALE_AFTER_MS,
            expire_after: EXPIRE_AFTER_MS,
            self_id: None,
        };
        Ok(build(&sessions, &params).only(&columns))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanban::commands::inbox::Claimed;
    use std::cell::RefCell;

    struct Copies(Vec<String>);

    impl RecordStore for Copies {
        fn path(&self, session: &str) -> Result<PathBuf, String> {
            Ok(PathBuf::from(format!("/r/{session}.json")))
        }
        fn read_all(&self) -> Result<Vec<String>, String> {
            Ok(self.0.clone())
        }
        fn remove(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct Inbox {
        cleared: RefCell<Vec<String>>,
    }

    impl InboxStore for Inbox {
        fn dir(&self, _: &str) -> Result<PathBuf, String> {
            Ok(PathBuf::new())
        }
        fn put(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn claim_all(&self, _: &str, _: u64) -> Result<Vec<Claimed>, String> {
            Ok(Vec::new())
        }
        fn clear(&self, session: &str) -> Result<(), String> {
            self.cleared.borrow_mut().push(session.to_string());
            Ok(())
        }
        fn put_permit(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn take_permit(&self, _: &str, _: &str, _: u64) -> Result<Option<Claimed>, String> {
            Ok(None)
        }
    }

    fn copy(id: &str) -> String {
        format!(
            r#"{{"v":1,"session_id":"{id}","cwd":"/w","started_at":1,"heartbeat_at":100,"events":[{{"name":"turn.start","at":100}}]}}"#
        )
    }

    fn query(columns: &[&str]) -> StatusQuery {
        StatusQuery {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            session: None,
            now: 101,
        }
    }

    #[test]
    fn unreadable_copies_are_left_out_of_the_board() {
        let store = Copies(vec![copy("a"), "garbage".to_string(), copy("b")]);
        let inbox = Inbox::default();
        let records = KanbanRecords {
            store: &store,
            inbox: &inbox,
        };

        let board = records.status(&query(&[])).expect("board");

        let total: usize = board.columns.iter().map(|(_, cards)| cards.len()).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn forget_clears_the_inbox_of_that_session() {
        let store = Copies(Vec::new());
        let inbox = Inbox::default();
        let records = KanbanRecords {
            store: &store,
            inbox: &inbox,
        };

        records.forget("s1");

        assert_eq!(*inbox.cleared.borrow(), vec!["s1".to_string()]);
    }
}
