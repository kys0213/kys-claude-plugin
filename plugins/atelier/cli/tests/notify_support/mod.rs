//! Shared test doubles for the notify blackbox tests: a recording command
//! runner plus map-backed env/fs fakes.
// Each integration-test binary compiles this module separately and uses only
// a subset of it, so unused-item lints are expected noise here.
#![allow(dead_code)]

use atelier::notify::config::{ConfigEnv, ConfigFs};
use atelier::notify::exec::CommandRunner;
use atelier::notify::types::{AskQuestionPayload, Question};
use std::cell::RefCell;
use std::collections::HashMap;

/// One recorded invocation: (argv, stdin, timeout_secs).
pub type Call = (Vec<String>, Option<String>, u64);

/// Runner stub recording every call; programs (argv[0]) listed in `fail`
/// return Err.
pub struct StubRunner {
    pub calls: RefCell<Vec<Call>>,
    fail: Vec<String>,
}

impl StubRunner {
    pub fn new(fail: &[&str]) -> Self {
        StubRunner {
            calls: RefCell::new(Vec::new()),
            fail: fail.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl CommandRunner for StubRunner {
    fn run(&self, argv: &[String], stdin: Option<&str>, timeout_secs: u64) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push((argv.to_vec(), stdin.map(|s| s.to_string()), timeout_secs));
        if argv.first().is_some_and(|p| self.fail.contains(p)) {
            return Err("spawn failed".to_string());
        }
        Ok(())
    }
}

pub struct MapEnv(HashMap<String, String>);

impl ConfigEnv for MapEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

pub struct MapFs(HashMap<String, String>);

impl ConfigFs for MapFs {
    fn read_file(&self, path: &str) -> Option<String> {
        self.0.get(path).cloned()
    }
}

pub fn env(pairs: &[(&str, &str)]) -> MapEnv {
    MapEnv(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

pub fn fs(pairs: &[(&str, &str)]) -> MapFs {
    MapFs(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

pub fn ask_payload() -> AskQuestionPayload {
    AskQuestionPayload {
        session_id: Some("s1".to_string()),
        cwd: Some("/work/repo".to_string()),
        questions: vec![Question {
            header: Some("Auth".to_string()),
            question: "Which auth method?".to_string(),
            options: vec!["OAuth".to_string(), "API key".to_string()],
            multi_select: false,
        }],
    }
}
