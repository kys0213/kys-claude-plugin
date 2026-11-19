//! Opened entries on disk: `<config>/atelier/kanban/records/<session>.opened`,
//! beside the record copies so a board watching that folder reads them again.

use crate::kanban::commands::inbox::config_dir;
use crate::kanban::core::opened::{claim, may_clear, Opened, Seen};
use crate::kanban::core::request::check_id;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const SUFFIX: &str = ".opened";

pub trait OpenedStore {
    /// `None` when there is no entry.
    fn read(&self, session: &str) -> Result<Option<String>, String>;
    fn write(&self, session: &str, contents: &str) -> Result<(), String>;
    /// Succeeds when there is no entry.
    fn remove(&self, session: &str) -> Result<(), String>;
}

pub struct FsOpened {
    root: Result<PathBuf, String>,
}

impl FsOpened {
    pub fn from_env() -> FsOpened {
        FsOpened {
            root: config_dir().map(|c| c.join("atelier").join("kanban").join("records")),
        }
    }

    fn path(&self, session: &str) -> Result<PathBuf, String> {
        Ok(self.root.clone()?.join(format!("{session}{SUFFIX}")))
    }
}

impl OpenedStore for FsOpened {
    fn read(&self, session: &str) -> Result<Option<String>, String> {
        match fs::read_to_string(self.path(session)?) {
            Ok(raw) => Ok(Some(raw)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("열린 터미널 항목을 읽지 못했어요: {e}")),
        }
    }

    /// Written whole through a hidden file, so a board reading at the same
    /// time never sees half an entry.
    fn write(&self, session: &str, contents: &str) -> Result<(), String> {
        let root = self.root.clone()?;
        let failed = |e: std::io::Error| format!("열린 터미널 항목을 쓰지 못했어요: {e}");
        fs::create_dir_all(&root).map_err(failed)?;
        let partial = root.join(format!(".{session}{SUFFIX}.{}", std::process::id()));
        fs::write(&partial, contents).map_err(failed)?;
        fs::rename(&partial, self.path(session)?).map_err(failed)
    }

    fn remove(&self, session: &str) -> Result<(), String> {
        match fs::remove_file(self.path(session)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("열린 터미널 항목을 지우지 못했어요: {e}")),
        }
    }
}

pub struct KanbanOpened<'a> {
    pub store: &'a dyn OpenedStore,
}

fn answer(result: Result<Value, String>) -> Value {
    result.unwrap_or_else(|e| json!({ "error": e }))
}

fn opened_json(session: &str, opened: &Opened) -> Value {
    json!({ "opened": { "session_id": session, "at": opened.at, "terminal": opened.terminal } })
}

impl KanbanOpened<'_> {
    /// An unreadable entry counts as none.
    pub fn get(&self, session: &str) -> Option<Opened> {
        self.store
            .read(session)
            .ok()
            .flatten()
            .and_then(|raw| Opened::parse(&raw))
    }

    pub fn open(&self, session: &str, seen: &str, now: u64) -> Value {
        answer((|| {
            check_id("session", session)?;
            let seen = Seen::parse(seen)?;
            let opened = claim(self.get(session).as_ref(), seen, now)?;
            self.store.write(session, &opened.to_file())?;
            Ok(opened_json(session, &opened))
        })())
    }

    pub fn report(&self, session: &str, terminal: &str, now: u64) -> Value {
        answer((|| {
            check_id("session", session)?;
            check_id("terminal", terminal)?;
            let opened = Opened::on(terminal, now);
            self.store.write(session, &opened.to_file())?;
            Ok(opened_json(session, &opened))
        })())
    }

    pub fn clear(&self, session: &str, seen: Option<&str>) -> Value {
        answer((|| {
            check_id("session", session)?;
            let seen = seen.map(Seen::parse).transpose()?;
            may_clear(self.get(session).as_ref(), seen)?;
            self.store.remove(session)?;
            Ok(json!({ "cleared": session }))
        })())
    }
}
