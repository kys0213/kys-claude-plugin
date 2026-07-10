//! Shared test doubles for the notify blackbox tests: recording stubs for the
//! three I/O ports plus map-backed env/fs fakes.

use atelier::notify::channel::Effects;
use atelier::notify::config::{ConfigEnv, ConfigFs};
use atelier::notify::types::{AskQuestionPayload, Question};
use std::cell::RefCell;
use std::collections::HashMap;

/// Poster stub recording every post; URLs listed in `fail` return Err.
pub struct StubPoster {
    pub posts: RefCell<Vec<(String, String)>>,
    fail: Vec<String>,
}

impl StubPoster {
    pub fn new(fail: &[&str]) -> Self {
        StubPoster {
            posts: RefCell::new(Vec::new()),
            fail: fail.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl atelier::notify::transport::HttpPoster for StubPoster {
    fn post_json(&self, url: &str, body: &str) -> Result<(), String> {
        self.posts
            .borrow_mut()
            .push((url.to_string(), body.to_string()));
        if self.fail.iter().any(|f| f == url) {
            return Err("boom".to_string());
        }
        Ok(())
    }
}

/// Appender stub recording every append; paths listed in `fail` return Err.
pub struct StubAppender {
    pub appends: RefCell<Vec<(String, String)>>,
    fail: Vec<String>,
}

impl StubAppender {
    pub fn new(fail: &[&str]) -> Self {
        StubAppender {
            appends: RefCell::new(Vec::new()),
            fail: fail.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl atelier::notify::transport::FileAppender for StubAppender {
    fn append_line(&self, path: &str, line: &str) -> Result<(), String> {
        self.appends
            .borrow_mut()
            .push((path.to_string(), line.to_string()));
        if self.fail.iter().any(|f| f == path) {
            return Err("disk full".to_string());
        }
        Ok(())
    }
}

/// Desktop stub recording every banner; `fail` makes it return Err.
pub struct StubDesktop {
    pub banners: RefCell<Vec<(String, String)>>,
    fail: bool,
}

impl StubDesktop {
    pub fn new(fail: bool) -> Self {
        StubDesktop {
            banners: RefCell::new(Vec::new()),
            fail,
        }
    }
}

impl atelier::notify::transport::DesktopNotifier for StubDesktop {
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        self.banners
            .borrow_mut()
            .push((title.to_string(), body.to_string()));
        if self.fail {
            return Err("no notifier".to_string());
        }
        Ok(())
    }
}

pub fn fx<'a>(
    poster: &'a StubPoster,
    appender: &'a StubAppender,
    desktop: &'a StubDesktop,
) -> Effects<'a> {
    Effects {
        poster,
        appender,
        desktop,
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
