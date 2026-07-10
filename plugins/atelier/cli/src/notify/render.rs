//! Deterministic event rendering for channel command templates. The CLI owns
//! this so pre-existing CLIs can be wired as channels without JSON parsing on
//! their side: config templates reference `{title}`, `{body}`, `{text}`,
//! `{json}`, `{cwd}`, `{event}`, and `{var#json}` yields the value as a JSON
//! string literal (quotes included) for embedding in JSON bodies.

use crate::notify::event::Event;

/// All template variables for an event, rendered once per delivery.
pub fn vars(event: &Event) -> Vec<(&'static str, String)> {
    vec![
        ("title", title(event)),
        ("body", body(event)),
        ("text", text(event)),
        ("json", event.structured_json()),
        ("cwd", event.cwd().unwrap_or("").to_string()),
        ("event", event.kind().to_string()),
    ]
}

/// Replaces `{name}` / `{name#json}` tokens with variable values. A `{` that
/// doesn't open a token-shaped `{ident}` (e.g. a literal JSON brace in
/// `{"text": {text#json}}`) passes through and scanning continues after it.
/// Unknown names and modifiers are left verbatim so a config typo is visible
/// in the delivered message instead of silently vanishing.
pub fn substitute(template: &str, vars: &[(&'static str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let token = after.find('}').map(|end| (&after[1..end], end));
        let is_token = token.is_some_and(|(t, _)| {
            !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '#')
        });
        if !is_token {
            out.push('{');
            rest = &after[1..];
            continue;
        }
        let (token, end) = token.unwrap();
        let (name, as_json) = match token.split_once('#') {
            Some((n, "json")) => (n, true),
            Some(_) => ("", false), // unknown modifier → leave verbatim
            None => (token, false),
        };
        match vars.iter().find(|(k, _)| *k == name) {
            Some((_, value)) if as_json => {
                out.push_str(&serde_json::to_string(value).unwrap_or_default())
            }
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&after[..=end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Short banner title per event kind.
fn title(event: &Event) -> String {
    match event {
        Event::AskQuestion(_) => "Claude 질문 대기".to_string(),
        Event::Notification(_) => "Claude 입력 대기".to_string(),
    }
}

/// Short banner body: banners truncate, so only the first question + a count
/// of the rest.
fn body(event: &Event) -> String {
    match event {
        Event::AskQuestion(p) => {
            let mut body = match p.questions.first() {
                Some(q) => q.question.clone(),
                None => String::new(),
            };
            if p.questions.len() > 1 {
                body.push_str(&format!(" (외 {}개)", p.questions.len() - 1));
            }
            if let Some(cwd) = &p.cwd {
                body.push_str(&format!("\n{cwd}"));
            }
            body
        }
        Event::Notification(p) => {
            let mut body = p.message.clone().unwrap_or_default();
            if let Some(cwd) = &p.cwd {
                body.push_str(&format!("\n{cwd}"));
            }
            body
        }
    }
}

/// Full human-readable message (markdown-lite, suitable for Slack-style
/// receivers).
fn text(event: &Event) -> String {
    match event {
        Event::AskQuestion(p) => {
            let mut text = String::from(":question: *Claude 세션이 응답을 기다리고 있습니다*\n");
            if let Some(cwd) = &p.cwd {
                text.push_str(&format!("프로젝트: `{cwd}`\n"));
            }
            for q in &p.questions {
                match &q.header {
                    Some(h) => text.push_str(&format!("\n*[{h}]* {}\n", q.question)),
                    None => text.push_str(&format!("\n*{}*\n", q.question)),
                }
                for opt in &q.options {
                    text.push_str(&format!("• {opt}\n"));
                }
            }
            text
        }
        Event::Notification(p) => {
            let mut text = String::from(":bell: *Claude 세션이 입력을 기다리고 있습니다*\n");
            if let Some(cwd) = &p.cwd {
                text.push_str(&format!("프로젝트: `{cwd}`\n"));
            }
            if let Some(message) = &p.message {
                text.push_str(&format!("\n{message}\n"));
            }
            text
        }
    }
}
