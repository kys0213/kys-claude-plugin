#![allow(dead_code)]

use anyhow::Result;
use autopilot::gh::GhOps;
use serde_json::Value;
use std::sync::Mutex;

/// A mock GhOps that returns predefined responses based on arg patterns.
pub struct MockGh {
    responses: Mutex<Vec<MockResponse>>,
    pub calls: Mutex<Vec<Vec<String>>>,
}

struct MockResponse {
    match_fn: Box<dyn Fn(&[&str]) -> bool + Send>,
    result: MockResult,
}

enum MockResult {
    Run(String),
    ListJson(Vec<Value>),
}

impl MockGh {
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Register: when any arg contains `pattern`, `list_json` returns `items`.
    pub fn on_list_containing(self, pattern: &str, items: Vec<Value>) -> Self {
        let pat = pattern.to_string();
        self.responses.lock().unwrap().push(MockResponse {
            match_fn: Box::new(move |args| args.iter().any(|a| a.contains(&pat))),
            result: MockResult::ListJson(items),
        });
        self
    }

    /// Register: when any arg contains `pattern`, `run` returns `output`.
    pub fn on_run_containing(self, pattern: &str, output: &str) -> Self {
        let pat = pattern.to_string();
        let out = output.to_string();
        self.responses.lock().unwrap().push(MockResponse {
            match_fn: Box::new(move |args| args.iter().any(|a| a.contains(&pat))),
            result: MockResult::Run(out),
        });
        self
    }

    fn record_call(&self, args: &[&str]) {
        self.calls
            .lock()
            .unwrap()
            .push(args.iter().map(|s| s.to_string()).collect());
    }

    fn find_response_run(&self, args: &[&str]) -> Result<String> {
        let responses = self.responses.lock().unwrap();
        for resp in responses.iter().rev() {
            if (resp.match_fn)(args) {
                return match &resp.result {
                    MockResult::Run(s) => Ok(s.clone()),
                    MockResult::ListJson(_) => Ok("[]".to_string()),
                };
            }
        }
        Ok(String::new())
    }

    fn find_response_list(&self, args: &[&str]) -> Result<Vec<Value>> {
        let responses = self.responses.lock().unwrap();
        for resp in responses.iter().rev() {
            if (resp.match_fn)(args) {
                return match &resp.result {
                    MockResult::ListJson(v) => Ok(v.clone()),
                    MockResult::Run(_) => Ok(vec![]),
                };
            }
        }
        Ok(vec![])
    }
}

impl GhOps for MockGh {
    fn run(&self, args: &[&str]) -> Result<String> {
        self.record_call(args);
        self.find_response_run(args)
    }

    fn list_json(&self, args: &[&str]) -> Result<Vec<Value>> {
        self.record_call(args);
        self.find_response_list(args)
    }
}
