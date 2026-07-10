//! PreToolUse payload parsing for the `AskUserQuestion` tool. Swallow-all
//! like `git::commands::guard::HookPayload`: any read/JSON failure yields an
//! empty payload so the hook path stays a silent no-op.

use crate::notify::types::{AskQuestionPayload, NotificationPayload, Question};
use serde_json::Value;

impl NotificationPayload {
    pub fn parse(raw: &str) -> NotificationPayload {
        let v: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => return NotificationPayload::default(),
        };
        NotificationPayload {
            session_id: v["session_id"].as_str().map(|s| s.to_string()),
            cwd: v["cwd"].as_str().map(|s| s.to_string()),
            message: v["message"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        }
    }
}

impl AskQuestionPayload {
    pub fn parse(raw: &str) -> AskQuestionPayload {
        let v: Value = match serde_json::from_str(raw) {
            Ok(v) => v,
            Err(_) => return AskQuestionPayload::default(),
        };
        AskQuestionPayload {
            session_id: v["session_id"].as_str().map(|s| s.to_string()),
            cwd: v["cwd"].as_str().map(|s| s.to_string()),
            questions: v["tool_input"]["questions"]
                .as_array()
                .map(|arr| arr.iter().filter_map(parse_question).collect())
                .unwrap_or_default(),
        }
    }
}

/// Parses one questions[] entry; entries without a `question` string are
/// dropped. Options accept both `{label}` objects and bare strings.
fn parse_question(v: &Value) -> Option<Question> {
    let question = v["question"].as_str()?.to_string();
    let options = v["options"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|o| o["label"].as_str().or_else(|| o.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    Some(Question {
        header: v["header"].as_str().map(|s| s.to_string()),
        question,
        options,
        multi_select: v["multiSelect"].as_bool().unwrap_or(false),
    })
}
