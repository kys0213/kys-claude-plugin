//! In-memory edges for the kanban request commands.
//!
//! `#![allow(dead_code)]` because each test binary uses only part of them
//! (Cargo compiles this module into every test crate that declares it).
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use atelier::kanban::commands::inbox::{Claimed, IdSource, InboxStore, Pause};
use atelier::kanban::commands::ledger_store::LedgerStore;
use atelier::kanban::commands::requests::KanbanRequests;
use atelier::kanban::commands::spawn::{Launched, Launcher, RepoProbe};
use atelier::kanban::core::allowlist::RealPaths;
use atelier::kanban::core::ledger::MonthFile;
use atelier::kanban::core::spawn::RepoPaths;

/// Requests by session and id; `age_ms` is what every claimed request reads as.
#[derive(Default)]
pub struct MemInbox {
    pub files: RefCell<BTreeMap<String, BTreeMap<String, String>>>,
    pub permits: RefCell<BTreeMap<(String, String), String>>,
    pub age_ms: RefCell<u64>,
}

impl MemInbox {
    pub fn ids(&self, session: &str) -> Vec<String> {
        self.files
            .borrow()
            .get(session)
            .map(|f| f.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn put_raw(&self, session: &str, id: &str, contents: &str) {
        self.files
            .borrow_mut()
            .entry(session.to_string())
            .or_default()
            .insert(id.to_string(), contents.to_string());
    }
}

impl InboxStore for MemInbox {
    fn dir(&self, session: &str) -> Result<PathBuf, String> {
        Ok(PathBuf::from("/mem/inbox").join(session))
    }

    fn put(&self, session: &str, id: &str, contents: &str) -> Result<(), String> {
        self.put_raw(session, id, contents);
        Ok(())
    }

    fn claim_all(&self, session: &str, _now: u64) -> Result<Vec<Claimed>, String> {
        let taken = self.files.borrow_mut().remove(session).unwrap_or_default();
        let age_ms = *self.age_ms.borrow();
        Ok(taken
            .into_iter()
            .map(|(id, contents)| Claimed {
                id,
                age_ms,
                contents: Some(contents),
            })
            .collect())
    }

    fn clear(&self, session: &str) -> Result<(), String> {
        self.files.borrow_mut().remove(session);
        Ok(())
    }

    fn put_permit(&self, session: &str, request: &str, contents: &str) -> Result<(), String> {
        self.permits.borrow_mut().insert(
            (session.to_string(), request.to_string()),
            contents.to_string(),
        );
        Ok(())
    }

    fn take_permit(
        &self,
        session: &str,
        request: &str,
        _now: u64,
    ) -> Result<Option<Claimed>, String> {
        let taken = self
            .permits
            .borrow_mut()
            .remove(&(session.to_string(), request.to_string()));
        Ok(taken.map(|contents| Claimed {
            id: request.to_string(),
            age_ms: *self.age_ms.borrow(),
            contents: Some(contents),
        }))
    }
}

/// Counts what it was asked to wait instead of waiting.
#[derive(Default)]
pub struct CountingPause {
    pub paused_ms: RefCell<u64>,
}

impl Pause for CountingPause {
    fn pause(&self, ms: u64) {
        *self.paused_ms.borrow_mut() += ms;
    }
}

/// `r1`, `r2`, ... in order.
#[derive(Default)]
pub struct CountingIds {
    next: RefCell<u32>,
}

impl IdSource for CountingIds {
    fn next_id(&self) -> String {
        let mut next = self.next.borrow_mut();
        *next += 1;
        format!("r{next}")
    }
}

/// Answers `paths` from a table keyed by cwd.
#[derive(Default)]
pub struct MapProbe {
    pub repos: BTreeMap<String, (String, String)>,
}

impl MapProbe {
    /// `cwd` inside the repository whose top is `top`, main git dir `<top>/.git`.
    pub fn with(mut self, cwd: &str, top: &str) -> MapProbe {
        self.repos
            .insert(cwd.to_string(), (top.to_string(), format!("{top}/.git")));
        self
    }

    pub fn with_git_dir(mut self, cwd: &str, top: &str, git_dir: &str) -> MapProbe {
        self.repos
            .insert(cwd.to_string(), (top.to_string(), git_dir.to_string()));
        self
    }
}

impl RepoProbe for MapProbe {
    fn paths(&self, cwd: &str) -> Option<RepoPaths> {
        let (top, git_dir) = self.repos.get(cwd)?;
        Some(RepoPaths {
            cwd: PathBuf::from(cwd),
            top: PathBuf::from(top),
            git_dir: PathBuf::from(git_dir),
        })
    }
}

/// Records each launch and answers with `out`.
pub struct FakeLauncher {
    pub out: Result<(bool, String, String), String>,
    pub launches: RefCell<Vec<(String, Vec<String>)>>,
}

impl FakeLauncher {
    pub fn printing(stdout: &str) -> FakeLauncher {
        FakeLauncher {
            out: Ok((true, stdout.to_string(), String::new())),
            launches: RefCell::new(Vec::new()),
        }
    }
}

impl Launcher for FakeLauncher {
    fn launch(&self, cwd: &str, args: &[String]) -> Result<Launched, String> {
        self.launches
            .borrow_mut()
            .push((cwd.to_string(), args.to_vec()));
        self.out.clone().map(|(success, stdout, stderr)| Launched {
            success,
            stdout,
            stderr,
        })
    }
}

/// Ledger lines by month.
#[derive(Default)]
pub struct MemLedger {
    pub months: RefCell<BTreeMap<String, Vec<String>>>,
}

impl LedgerStore for MemLedger {
    fn append(&self, month: &str, lines: &[String]) -> Result<(), String> {
        self.months
            .borrow_mut()
            .entry(month.to_string())
            .or_default()
            .extend(lines.iter().cloned());
        Ok(())
    }

    fn read(&self) -> Result<Vec<MonthFile>, String> {
        Ok(self
            .months
            .borrow()
            .iter()
            .map(|(month, lines)| MonthFile {
                month: month.clone(),
                lines: lines.clone(),
            })
            .collect())
    }

    fn rewrite(
        &self,
        change: &mut dyn FnMut(Vec<MonthFile>) -> Vec<MonthFile>,
    ) -> Result<(), String> {
        let kept = change(self.read()?);
        *self.months.borrow_mut() = kept.into_iter().map(|m| (m.month, m.lines)).collect();
        Ok(())
    }
}

/// Every path is where it is named.
pub struct NamedPaths;

impl RealPaths for NamedPaths {
    fn real(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }
}

pub struct Edges {
    pub inbox: MemInbox,
    pub pause: CountingPause,
    pub ids: CountingIds,
    pub probe: MapProbe,
    pub launcher: FakeLauncher,
    pub ledger: MemLedger,
}

impl Default for Edges {
    fn default() -> Edges {
        Edges {
            inbox: MemInbox::default(),
            pause: CountingPause::default(),
            ids: CountingIds::default(),
            probe: MapProbe::default(),
            launcher: FakeLauncher::printing("backgrounded · abc123\n"),
            ledger: MemLedger::default(),
        }
    }
}

impl Edges {
    pub fn requests(&self) -> KanbanRequests<'_> {
        KanbanRequests {
            inbox: &self.inbox,
            pause: &self.pause,
            ids: &self.ids,
            probe: &self.probe,
            launcher: &self.launcher,
            ledger: &self.ledger,
            paths: &NamedPaths,
        }
    }
}
