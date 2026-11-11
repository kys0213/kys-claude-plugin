//! What a waiting session waits on, as the delegate reads it: the content,
//! each decision's key, and whether the wait is of a kind a precedent may
//! answer, with every reason it is not. Text is given as recorded; keys are
//! computed from it.

use super::allowlist::{check_permission, RealPaths, Refusal, Scope};
use super::board::{PermissionAsk, WaitOn, Waiting};
use super::detail::{prompt_lines, reply_lines, Decision, WAIT_FIELDS};
use super::matchkey::{decision_key, permission_key, permission_target, question_key, sentence};
use super::progress::first_field;
use super::question::{Ask, Question};
use super::text::fold;

/// Why a wait is not one a precedent may answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    MultiSelect,
    /// A text the wait depends on was cut when recorded, or its length was
    /// not recorded.
    Truncated,
    Refused(Refusal),
    Irreversible,
    Decision,
    Input,
    NoKey,
}

impl Blocked {
    pub fn id(&self) -> String {
        match self {
            Blocked::MultiSelect => "multi_select".to_string(),
            Blocked::Truncated => "truncated".to_string(),
            Blocked::Refused(Refusal::Compound) => "compound".to_string(),
            Blocked::Refused(Refusal::Expansion) => "expansion".to_string(),
            Blocked::Refused(Refusal::NotAllowlisted(word)) => format!("not_allowlisted:{word}"),
            Blocked::Refused(Refusal::OutsideRepo) => "outside_repo".to_string(),
            Blocked::Irreversible => "guard:irreversible".to_string(),
            Blocked::Decision => "decision".to_string(),
            Blocked::Input => "input".to_string(),
            Blocked::NoKey => "no_key".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionWait {
    pub question: Question,
    pub key: Option<String>,
    pub key_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionWait {
    pub tool: String,
    /// `<tool>: <what it works on>`, or the tool alone.
    pub target: String,
    pub truncated: bool,
    pub key: Option<String>,
    pub key_label: Option<String>,
    /// The request passes the allow list inside the repository.
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionWait {
    pub ask: String,
    pub recap: Option<String>,
    pub options: Vec<String>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Question(Vec<QuestionWait>),
    Permission(PermissionWait),
    Decision(DecisionWait),
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wait {
    pub id: Option<String>,
    pub since: u64,
    /// The person's latest prompt, folded and cut.
    pub request: Option<String>,
    /// The first lines of the reply just before the wait.
    pub context: Option<String>,
    pub subject: Subject,
    pub blocked: Vec<Blocked>,
}

impl Wait {
    pub fn kind(&self) -> &'static str {
        match self.subject {
            Subject::Question(_) => "question",
            Subject::Permission(_) => "permission",
            Subject::Decision(_) => "decision",
            Subject::Input => "input",
        }
    }

    pub fn proxyable(&self) -> bool {
        self.blocked.is_empty()
    }
}

/// The session a wait belongs to, as far as judging it needs.
pub struct Place<'a> {
    pub repo_root: Option<&'a str>,
    pub cwd: &'a str,
    pub permission_mode: Option<&'a str>,
}

impl Place<'_> {
    pub fn repo(&self) -> &str {
        self.repo_root.unwrap_or(self.cwd)
    }
}

fn add_if(blocked: &mut Vec<Blocked>, cond: bool, reason: Blocked) {
    if cond && !blocked.contains(&reason) {
        blocked.push(reason);
    }
}

/// Words that point at acts hard to take back, matched as case-insensitive
/// substrings of the whitespace-folded text, so they also hit inside longer
/// words.
const IRREVERSIBLE: [&str; 15] = [
    "force",
    "push -f",
    "reset --hard",
    "merge",
    "deploy",
    "release",
    "publish",
    "drop",
    "rm -rf",
    "delete",
    "배포",
    "삭제",
    "강제",
    "머지",
    "릴리스",
];

fn irreversible<'a>(texts: impl IntoIterator<Item = &'a str>) -> bool {
    texts.into_iter().any(|text| {
        let folded = fold(text).to_lowercase();
        IRREVERSIBLE.iter().any(|marker| folded.contains(marker))
    })
}

/// The mod cuts by UTF-16 code units, as JavaScript counts a string.
fn was_cut(stored: &str, original: u64) -> bool {
    (stored.encode_utf16().count() as u64) < original
}

fn question_cut(ask: &Ask, index: usize, question: &Question) -> bool {
    let Some(lengths) = &ask.orig_len else {
        return true;
    };
    let text_cut = lengths
        .question
        .get(index)
        .is_none_or(|len| was_cut(&question.question, *len));
    let labels_cut = match lengths.labels.get(index) {
        Some(labels) if labels.len() == question.options.len() => question
            .options
            .iter()
            .zip(labels)
            .any(|(option, len)| was_cut(&option.label, *len)),
        _ => true,
    };
    text_cut || labels_cut
}

fn question_wait(ask: &Ask, place: &Place, blocked: &mut Vec<Blocked>) -> Subject {
    let waits = ask
        .questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            let cut = question_cut(ask, index, question);
            let texts = std::iter::once(question.question.as_str())
                .chain(std::iter::once(question.header.as_str()))
                .chain(question.options.iter().flat_map(|o| {
                    std::iter::once(o.label.as_str()).chain(o.description.as_deref())
                }));
            let key = (!cut).then(|| question_key(place.repo(), question));
            add_if(blocked, question.multi_select, Blocked::MultiSelect);
            add_if(blocked, cut, Blocked::Truncated);
            add_if(blocked, irreversible(texts), Blocked::Irreversible);
            add_if(blocked, key.is_none(), Blocked::NoKey);
            QuestionWait {
                question: question.clone(),
                key,
                key_label: sentence(&question.question),
            }
        })
        .collect();
    Subject::Question(waits)
}

fn permission_cut(ask: &PermissionAsk) -> bool {
    let Some(lengths) = &ask.input_len else {
        return true;
    };
    let fields_cut = ask.request.input.iter().any(|(name, value)| {
        lengths
            .iter()
            .find(|(n, _)| n == name)
            .is_none_or(|(_, len)| was_cut(value, *len))
    });
    let unrecorded = lengths
        .iter()
        .any(|(name, _)| !ask.request.input.iter().any(|(n, _)| n == name));
    fields_cut || unrecorded
}

fn permission_wait(
    ask: &PermissionAsk,
    place: &Place,
    paths: &dyn RealPaths,
    blocked: &mut Vec<Blocked>,
) -> Subject {
    let scope = Scope {
        root: place.repo_root,
        cwd: place.cwd,
        paths,
    };
    let tool = ask.request.tool.as_str();
    let input = &ask.request.input;
    let truncated = permission_cut(ask);
    let refusals = if truncated {
        Vec::new()
    } else {
        check_permission(tool, input, &scope)
    };
    let target = (!truncated)
        .then(|| permission_target(tool, input, &scope))
        .flatten();
    let key = target.as_ref().map(|target| {
        permission_key(
            place.repo(),
            place.permission_mode.unwrap_or(""),
            tool,
            target,
        )
    });
    add_if(blocked, truncated, Blocked::Truncated);
    for refusal in &refusals {
        add_if(blocked, true, Blocked::Refused(refusal.clone()));
    }
    add_if(blocked, key.is_none(), Blocked::NoKey);
    Subject::Permission(PermissionWait {
        tool: tool.to_string(),
        target: match first_field(input, &WAIT_FIELDS) {
            Some((_, value)) => format!("{tool}: {value}"),
            None => tool.to_string(),
        },
        truncated,
        key_label: target.map(|target| format!("{tool} {target}")),
        key,
        read_only: !truncated && refusals.is_empty(),
    })
}

fn decision_cut(decision: &Decision) -> bool {
    let Some(lengths) = &decision.orig_len else {
        return true;
    };
    was_cut(&decision.ask, lengths.ask)
        || lengths.options.len() != decision.options.len()
        || decision
            .options
            .iter()
            .zip(&lengths.options)
            .any(|(option, len)| was_cut(option, *len))
}

fn decision_wait(decision: &Decision, place: &Place, blocked: &mut Vec<Blocked>) -> Subject {
    let cut = decision_cut(decision);
    let key = (!cut).then(|| decision_key(place.repo(), &decision.ask, &decision.options));
    let texts =
        std::iter::once(decision.ask.as_str()).chain(decision.options.iter().map(String::as_str));
    add_if(blocked, true, Blocked::Decision);
    add_if(blocked, cut, Blocked::Truncated);
    add_if(blocked, irreversible(texts), Blocked::Irreversible);
    add_if(blocked, key.is_none(), Blocked::NoKey);
    Subject::Decision(DecisionWait {
        ask: decision.ask.clone(),
        recap: decision.recap.clone(),
        options: decision.options.clone(),
        key,
    })
}

pub fn derive(waiting: &Waiting, place: &Place, paths: &dyn RealPaths) -> Wait {
    let mut blocked = Vec::new();
    let (id, subject) = match &waiting.on {
        WaitOn::Question(ask) => (
            Some(ask.tool_use_id.clone()),
            question_wait(ask, place, &mut blocked),
        ),
        WaitOn::Permission(ask) => (
            ask.id.clone(),
            permission_wait(ask, place, paths, &mut blocked),
        ),
        WaitOn::Decision(decision) => (
            Some(decision.at.to_string()),
            decision_wait(decision, place, &mut blocked),
        ),
        WaitOn::Input => {
            blocked.push(Blocked::Input);
            (None, Subject::Input)
        }
    };
    let context = reply_lines(waiting.context.as_deref());
    Wait {
        id,
        since: waiting.since,
        request: prompt_lines(waiting.request.as_deref()).into_iter().next(),
        context: (!context.is_empty()).then(|| context.join("\n")),
        subject,
        blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_match_in_any_case_and_spacing() {
        assert!(irreversible(["Git PUSH   -f origin"]));
        assert!(irreversible(["fine", "Deleted files"]));
        assert!(!irreversible(["Which color?", "Red"]));
    }

    #[test]
    fn lengths_compare_in_utf16_code_units() {
        assert!(!was_cut("😀", 2));
        assert!(was_cut("😀", 3));
        assert!(!was_cut("가", 1));
    }
}
