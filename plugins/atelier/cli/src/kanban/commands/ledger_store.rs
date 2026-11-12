//! The ledger on disk: `<config>/atelier/kanban/ledger/<YYYY-MM>.jsonl`, one
//! entry per line. Every write holds an exclusive lock on `.lock` beside the
//! month files, so lines added at once never mix and a prune never loses one.

use crate::kanban::commands::inbox::{config_dir, replace_file};
use crate::kanban::core::ledger::MonthFile;
use crate::kanban::core::precedent::Book;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const SUFFIX: &str = ".jsonl";
const LOCK: &str = ".lock";

pub trait LedgerStore {
    /// Adds the lines to the month's file, all under one hold of the lock.
    fn append(&self, month: &str, lines: &[String]) -> Result<(), String>;
    /// Every month file, empty when the ledger was never written.
    fn read(&self) -> Result<Vec<MonthFile>, String>;
    fn read_from(&self, month: &str) -> Result<Vec<MonthFile>, String> {
        let mut months = self.read()?;
        months.retain(|m| m.month.as_str() >= month);
        Ok(months)
    }
    /// Under the lock: reads every month, writes back what `change` keeps,
    /// and removes the months it leaves out.
    fn rewrite(
        &self,
        change: &mut dyn FnMut(Vec<MonthFile>) -> Vec<MonthFile>,
    ) -> Result<(), String>;
}

pub fn read_book(store: &dyn LedgerStore) -> Result<Book, String> {
    let months = store.read()?;
    Ok(Book::read(
        months.iter().flat_map(|m| &m.lines).map(String::as_str),
    ))
}

pub struct FsLedger {
    root: Result<PathBuf, String>,
}

impl FsLedger {
    pub fn from_env() -> FsLedger {
        FsLedger {
            root: config_dir().map(|c| c.join("atelier").join("kanban").join("ledger")),
        }
    }

    fn locked<T>(&self, run: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
        let dir = self.root.clone()?;
        fs::create_dir_all(&dir).map_err(|e| format!("장부 디렉터리를 만들지 못했어요: {e}"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK))
            .map_err(|e| format!("장부 잠금 파일을 열지 못했어요: {e}"))?;
        lock.lock()
            .map_err(|e| format!("장부를 잠그지 못했어요: {e}"))?;
        let out = run(&dir);
        let _ = lock.unlock();
        out
    }
}

fn month_path(dir: &Path, month: &str) -> PathBuf {
    dir.join(format!("{month}{SUFFIX}"))
}

fn read_months(dir: &Path, from: &str) -> Result<Vec<MonthFile>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("장부를 읽지 못했어요 ({}): {e}", dir.display())),
    };
    let mut months: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.'))
        .filter_map(|name| name.strip_suffix(SUFFIX).map(str::to_string))
        .filter(|month| month.as_str() >= from)
        .collect();
    months.sort();
    months
        .into_iter()
        .map(|month| {
            let contents = fs::read(month_path(dir, &month))
                .map_err(|e| format!("장부 {month} 를 읽지 못했어요: {e}"))?;
            Ok(MonthFile {
                lines: String::from_utf8_lossy(&contents)
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(str::to_string)
                    .collect(),
                month,
            })
        })
        .collect()
}

fn write_month(dir: &Path, month: &MonthFile) -> Result<(), String> {
    let mut contents = month.lines.join("\n");
    contents.push('\n');
    replace_file(&month_path(dir, &month.month), &contents)
        .map_err(|e| format!("장부 {} 를 쓰지 못했어요: {e}", month.month))
}

impl LedgerStore for FsLedger {
    fn append(&self, month: &str, lines: &[String]) -> Result<(), String> {
        self.locked(|dir| {
            let mut file: File = OpenOptions::new()
                .create(true)
                .append(true)
                .open(month_path(dir, month))
                .map_err(|e| format!("장부 {month} 를 열지 못했어요: {e}"))?;
            let block: String = lines.iter().map(|line| format!("{line}\n")).collect();
            file.write_all(block.as_bytes())
                .map_err(|e| format!("장부 {month} 에 쓰지 못했어요: {e}"))
        })
    }

    fn read(&self) -> Result<Vec<MonthFile>, String> {
        read_months(&self.root.clone()?, "")
    }

    fn read_from(&self, month: &str) -> Result<Vec<MonthFile>, String> {
        read_months(&self.root.clone()?, month)
    }

    fn rewrite(
        &self,
        change: &mut dyn FnMut(Vec<MonthFile>) -> Vec<MonthFile>,
    ) -> Result<(), String> {
        self.locked(|dir| {
            let before = read_months(dir, "")?;
            let after = change(before.clone());
            for month in &before {
                match after.iter().find(|m| m.month == month.month) {
                    Some(kept) if kept == month => {}
                    Some(kept) => write_month(dir, kept)?,
                    None => fs::remove_file(month_path(dir, &month.month))
                        .map_err(|e| format!("장부 {} 를 지우지 못했어요: {e}", month.month))?,
                }
            }
            Ok(())
        })
    }
}
