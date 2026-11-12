//! Precedents: whether a person's own past decisions under one decision key
//! let the main agent answer a wait of that key, and whether an answer sent on
//! named precedents is the one they took. The sending CLI and the session that
//! takes the answer judge with the same `vouch`; the taker also hands it the
//! wait as the asking call holds it, whole.

use super::allowlist::RealPaths;
use super::board::{PermissionAsk, WaitOn, Waiting};
use super::ledger::{DecidedBy, Entry, EntryKind, DAY_MS};
use super::mask::mask;
use super::matchkey::{answer_fp, digest, key_prefixes, kind_of_prefix, KEY_HEX_LEN};
use super::progress::Activity;
use super::prompt::decides_as_person;
use super::question::{Ask, AskLengths, Question};
use super::relay::{Relay, RelayUse};
use super::request::{check_id, Verdict};
use super::text::{cut, js_len};
use super::wait::{derive, Place, Subject, Wait};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const VERSION: u64 = 1;
pub const WINDOW_MS: u64 = 90 * DAY_MS;
const MIN_DIRECT: usize = 2;
const MIN_WITH_APPROVED: usize = 3;
const DIRECT_TO_REENABLE: usize = 2;
const DEMOTE_WINDOW_MS: u64 = 7 * DAY_MS;
const REVOKES_TO_DEMOTE: usize = 2;
pub const MAX_PRECEDENTS: usize = 5;
pub const MAX_REASON_CHARS: usize = 200;
const ID_BYTES: usize = 4;

/// A decision key as `ledger record` makes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionKey {
    text: String,
    kind: EntryKind,
}

impl DecisionKey {
    pub fn parse(raw: &str) -> Result<DecisionKey, String> {
        let refuse = || {
            format!(
                "결정 키 `{raw}` 는 `{}` 와 {KEY_HEX_LEN}자리 16진수예요",
                key_prefixes().join("`·`")
            )
        };
        let (prefix, digits) = raw.split_once(':').ok_or_else(refuse)?;
        let kind = kind_of_prefix(prefix).ok_or_else(refuse)?;
        let hex = digits.len() == KEY_HEX_LEN
            && digits
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        if !hex {
            return Err(refuse());
        }
        Ok(DecisionKey {
            text: raw.to_string(),
            kind,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn kind(&self) -> EntryKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Shadow,
    Act,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Mode::Shadow => "shadow",
            Mode::Act => "act",
        }
    }

    fn parse(raw: &str) -> Option<Mode> {
        match raw {
            "shadow" => Some(Mode::Shadow),
            "act" => Some(Mode::Act),
            _ => None,
        }
    }
}

/// The kinds of wait the main agent may ever answer on precedents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proxied {
    Question,
    Permission,
}

impl Proxied {
    pub fn kind(self) -> EntryKind {
        match self {
            Proxied::Question => EntryKind::Question,
            Proxied::Permission => EntryKind::Permission,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RevokeTag {
    Revoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SettingTag {
    Setting,
}

/// A person turned a proxy answer over: its key is off until new direct
/// decisions agree again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revoke {
    v: u64,
    #[serde(rename = "type")]
    tag: RevokeTag,
    pub id: String,
    pub at: u64,
    pub entry: String,
    pub key: String,
    pub kind: EntryKind,
    pub reason: String,
}

impl Revoke {
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a revoke serializes")
    }
}

/// A delegate setting a person's slash command left: `scope` a kind or
/// `delegate`, `value` `act`, `shadow`, `on` or `off`; `session` names the
/// session a `delegate` setting turned on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setting {
    v: u64,
    #[serde(rename = "type")]
    tag: SettingTag,
    pub id: String,
    pub at: u64,
    pub scope: String,
    pub value: String,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Setting {
    pub fn new(at: u64, scope: &str, value: &str, origin: &str, session: Option<&str>) -> Setting {
        let suffix = digest(
            &[scope, value, origin, session.unwrap_or(""), &at.to_string()],
            ID_BYTES,
        );
        Setting {
            v: VERSION,
            tag: SettingTag::Setting,
            id: format!("S{at}-{suffix}"),
            at,
            scope: scope.to_string(),
            value: value.to_string(),
            origin: origin.to_string(),
            session: session.map(str::to_string),
        }
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("a setting serializes")
    }
}

/// The ledger as precedents read it; lines of no kind this version reads are
/// left out.
#[derive(Debug, Clone, Default)]
pub struct Book {
    entries: Vec<Entry>,
    revokes: Vec<Revoke>,
    settings: Vec<Setting>,
    relay_uses: Vec<RelayUse>,
}

impl Book {
    pub fn read<'a>(lines: impl IntoIterator<Item = &'a str>) -> Book {
        let mut book = Book::default();
        for line in lines {
            if let Some(entry) = Entry::parse(line) {
                book.entries.push(entry);
            } else if let Some(revoke) = serde_json::from_str::<Revoke>(line)
                .ok()
                .filter(|r| r.v == VERSION)
            {
                book.revokes.push(revoke);
            } else if let Some(setting) = serde_json::from_str::<Setting>(line)
                .ok()
                .filter(|s| s.v == VERSION)
            {
                book.settings.push(setting);
            } else if let Some(used) = RelayUse::parse(line) {
                book.relay_uses.push(used);
            }
        }
        book
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn relay_uses(&self) -> &[RelayUse] {
        &self.relay_uses
    }

    pub fn revokes(&self) -> &[Revoke] {
        &self.revokes
    }

    /// What the ledger held of `entry`'s key before `entry` was added: the
    /// key's earlier entries and revokes.
    pub fn before(&self, entry: &Entry) -> Book {
        let key = entry.key.as_deref();
        let earlier = (entry.at, entry.id.as_str());
        Book {
            entries: self
                .entries
                .iter()
                .filter(|e| e.key.as_deref() == key && (e.at, e.id.as_str()) < earlier)
                .cloned()
                .collect(),
            revokes: self
                .revokes
                .iter()
                .filter(|r| Some(r.key.as_str()) == key && r.at < entry.at)
                .cloned()
                .collect(),
            settings: Vec::new(),
            relay_uses: Vec::new(),
        }
    }
}

fn seen(at: u64, now: u64) -> bool {
    at <= now
}

pub fn in_window(entry: &Entry, now: u64) -> bool {
    seen(entry.at, now) && entry.at >= now.saturating_sub(WINDOW_MS)
}

pub fn is_human(entry: &Entry) -> bool {
    entry.decided_by == Some(DecidedBy::Human)
}

/// A person's own pick of one of the offered answers, kept as it was given.
fn supports(entry: &Entry) -> bool {
    is_human(entry) && !entry.free_text && !entry.answer_masked && !entry.inferred
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Support {
    pub direct: usize,
    pub approved: usize,
}

/// Why a key is not eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortfall {
    Support,
    Conflict,
    Disabled,
}

impl Shortfall {
    pub fn id(self) -> &'static str {
        match self {
            Shortfall::Support => "support",
            Shortfall::Conflict => "conflict",
            Shortfall::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub key: String,
    /// The key's precedents allow an answer, whatever its kind's mode.
    pub eligible: bool,
    /// The answer most human decisions in the window gave, as its newest
    /// supporting decision kept it.
    pub answer: Option<String>,
    pub answer_fp: Option<String>,
    pub support: Support,
    /// Human decisions in the window that answered otherwise.
    pub conflicts: usize,
    pub disabled: bool,
    pub mode: Mode,
    pub reasons: Vec<Shortfall>,
    /// When eligible, the newest supporting decisions of `answer`, at most
    /// `MAX_PRECEDENTS`: ids an agent answer may be sent on.
    pub precedent_ids: Vec<String>,
}

fn newest<'a, T>(items: impl Iterator<Item = &'a T>, at: fn(&T) -> (u64, &str)) -> Option<&'a T>
where
    T: 'a,
{
    items.max_by(|a, b| at(a).cmp(&at(b)))
}

pub fn disabled(book: &Book, key: &str, now: u64) -> bool {
    let Some(revoke) = newest(
        book.revokes
            .iter()
            .filter(|r| r.key == key && seen(r.at, now)),
        |r| (r.at, r.id.as_str()),
    ) else {
        return false;
    };
    let mut agreeing: HashMap<&str, usize> = HashMap::new();
    for entry in book.entries.iter().filter(|e| {
        e.key.as_deref() == Some(key)
            && e.at > revoke.at
            && in_window(e, now)
            && supports(e)
            && !e.approved
    }) {
        *agreeing.entry(&entry.answer_fp).or_default() += 1;
    }
    !agreeing.values().any(|n| *n >= DIRECT_TO_REENABLE)
}

/// A person's latest `act` or `shadow` for a kind, when it is `act`.
fn latest_act(book: &Book, kind: EntryKind, now: u64) -> Option<&Setting> {
    newest(
        book.settings.iter().filter(|s| {
            s.scope == kind.id()
                && seen(s.at, now)
                && decides_as_person(&s.origin)
                && Mode::parse(&s.value).is_some()
        }),
        |s| (s.at, s.id.as_str()),
    )
    .filter(|s| Mode::parse(&s.value) == Some(Mode::Act))
}

/// Whether proxy answers of a kind were turned over twice within a week
/// after `since`.
fn revoked_twice(book: &Book, kind: EntryKind, since: u64, now: u64) -> bool {
    let mut revoked: Vec<u64> = book
        .revokes
        .iter()
        .filter(|r| r.kind == kind && r.at > since && seen(r.at, now))
        .map(|r| r.at)
        .collect();
    revoked.sort_unstable();
    revoked
        .windows(REVOKES_TO_DEMOTE)
        .any(|w| w[REVOKES_TO_DEMOTE - 1] - w[0] < DEMOTE_WINDOW_MS)
}

/// Whether a kind a person set to `act` is back in `shadow` for its revokes.
pub fn demoted(book: &Book, kind: EntryKind, now: u64) -> bool {
    kind != EntryKind::Decision
        && latest_act(book, kind, now).is_some_and(|s| revoked_twice(book, kind, s.at, now))
}

/// Whether a kind answers on precedents now: a person's latest `act` for it,
/// not turned over twice within any week since. A decision never does.
pub fn mode(book: &Book, kind: EntryKind, now: u64) -> Mode {
    if kind == EntryKind::Decision {
        return Mode::Shadow;
    }
    match latest_act(book, kind, now) {
        Some(setting) if !revoked_twice(book, kind, setting.at, now) => Mode::Act,
        _ => Mode::Shadow,
    }
}

/// What the ledger says of `key` at `now`.
pub fn judge(book: &Book, key: &DecisionKey, now: u64) -> Match {
    let humans: Vec<&Entry> = book
        .entries
        .iter()
        .filter(|e| e.key.as_deref() == Some(key.as_str()) && is_human(e) && in_window(e, now))
        .collect();
    let support_of = |approved: bool| {
        humans
            .iter()
            .filter(|e| supports(e) && e.approved == approved)
            .count()
    };
    let support = Support {
        direct: support_of(false),
        approved: support_of(true),
    };
    let mut answers: HashMap<&str, (usize, (u64, &str))> = HashMap::new();
    for entry in &humans {
        let (count, latest) = answers.entry(&entry.answer_fp).or_default();
        *count += 1;
        *latest = (*latest).max((entry.at, entry.id.as_str()));
    }
    let (most, agreeing) = answers
        .iter()
        .max_by_key(|(_, counted)| **counted)
        .map_or((None, 0), |(fp, (count, _))| (Some(*fp), *count));
    let conflicts = humans.len() - agreeing;
    let mut backing: Vec<&Entry> = humans
        .iter()
        .copied()
        .filter(|e| supports(e) && Some(e.answer_fp.as_str()) == most)
        .collect();
    backing.sort_by(|a, b| (b.at, b.id.as_str()).cmp(&(a.at, a.id.as_str())));
    let precedent = backing.first().copied();
    let disabled = disabled(book, key.as_str(), now);
    let mut reasons = Vec::new();
    if support.direct < MIN_DIRECT && support.direct + support.approved < MIN_WITH_APPROVED {
        reasons.push(Shortfall::Support);
    }
    if conflicts > 0 {
        reasons.push(Shortfall::Conflict);
    }
    if disabled {
        reasons.push(Shortfall::Disabled);
    }
    let eligible = reasons.is_empty();
    let precedent_ids = if eligible {
        backing
            .iter()
            .take(MAX_PRECEDENTS)
            .map(|e| e.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    Match {
        key: key.as_str().to_string(),
        eligible,
        answer: precedent.map(|e| e.answer.clone()),
        answer_fp: precedent.map(|e| e.answer_fp.clone()),
        support,
        conflicts,
        disabled,
        mode: mode(book, key.kind(), now),
        reasons,
        precedent_ids,
    }
}

/// One ledger entry an answer is sent on, for question `q` (1-based; a
/// permission's is 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precedent {
    pub q: usize,
    pub id: String,
}

impl Precedent {
    /// `<q>:<entry id>`.
    pub fn parse(raw: &str) -> Result<Precedent, String> {
        let refuse = || format!("--precedent 은 `<질문 순번>:<장부 id>` 예요 (받은 값: {raw})");
        let (q, id) = raw.split_once(':').ok_or_else(refuse)?;
        let q: usize = q.parse().map_err(|_| refuse())?;
        if q == 0 {
            return Err(refuse());
        }
        check_id("ledger entry", id)?;
        Ok(Precedent {
            q,
            id: id.to_string(),
        })
    }
}

/// The precedents an agent answer is sent on, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precedents {
    precedents: Vec<Precedent>,
    reason: String,
}

/// What an agent answer is sent on: a person's precedents, or the answer a
/// person gave when the main agent asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    Precedents(Precedents),
    Relay(Relay),
}

impl Auth {
    pub fn precedents(&self) -> Option<&Precedents> {
        match self {
            Auth::Precedents(precedents) => Some(precedents),
            Auth::Relay(_) => None,
        }
    }

    pub fn relay(&self) -> Option<&Relay> {
        match self {
            Auth::Relay(relay) => Some(relay),
            Auth::Precedents(_) => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Auth::Precedents(precedents) => precedents.to_json(),
            Auth::Relay(relay) => relay.to_json(),
        }
    }

    pub fn from_json(value: &Value) -> Option<Auth> {
        if value.get("relay").is_some() {
            Relay::from_json(value).map(Auth::Relay)
        } else {
            Precedents::from_json(value).map(Auth::Precedents)
        }
    }
}

/// One answer and the entries named for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim<'a> {
    pub answer: &'a str,
    pub precedents: Vec<&'a str>,
}

impl Precedents {
    pub fn new(precedents: Vec<Precedent>, reason: &str) -> Result<Precedents, String> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err("근거(--reason)가 비어 있어요".to_string());
        }
        if reason.chars().count() > MAX_REASON_CHARS {
            return Err(format!("근거(--reason)가 {MAX_REASON_CHARS}자를 넘어요"));
        }
        if precedents.is_empty() {
            return Err("선례(--precedent)가 없어요".to_string());
        }
        let mut per_question: HashMap<usize, usize> = HashMap::new();
        for p in &precedents {
            *per_question.entry(p.q).or_default() += 1;
        }
        if let Some((q, _)) = per_question.iter().find(|(_, n)| **n > MAX_PRECEDENTS) {
            return Err(format!("{q}번 질문의 선례가 {MAX_PRECEDENTS}개를 넘어요"));
        }
        Ok(Precedents {
            precedents,
            reason: reason.to_string(),
        })
    }

    pub fn precedents(&self) -> &[Precedent] {
        &self.precedents
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The entry ids named for question `q`, in the order given.
    pub fn ids_for(&self, q: usize) -> Vec<&str> {
        self.precedents
            .iter()
            .filter(|p| p.q == q)
            .map(|p| p.id.as_str())
            .collect()
    }

    /// One claim per answer, in order; every answer needs precedents and
    /// every precedent an answer.
    pub fn claims<'a>(&'a self, answers: &'a [String]) -> Result<Vec<Claim<'a>>, String> {
        if let Some(p) = self.precedents.iter().find(|p| p.q > answers.len()) {
            return Err(format!(
                "{}번 질문은 없어요 — 답이 {}개예요",
                p.q,
                answers.len()
            ));
        }
        answers
            .iter()
            .enumerate()
            .map(|(index, answer)| {
                let precedents = self.ids_for(index + 1);
                if precedents.is_empty() {
                    return Err(format!("{}번 질문의 선례가 없어요", index + 1));
                }
                Ok(Claim {
                    answer: answer.as_str(),
                    precedents,
                })
            })
            .collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "precedents": self
                .precedents
                .iter()
                .map(|p| json!({ "q": p.q, "id": p.id }))
                .collect::<Vec<_>>(),
            "reason": self.reason,
        })
    }

    pub fn from_json(value: &Value) -> Option<Precedents> {
        let precedents = value
            .get("precedents")?
            .as_array()?
            .iter()
            .map(|p| {
                let q = usize::try_from(p.get("q")?.as_u64()?).ok()?;
                let id = p.get("id")?.as_str()?;
                Precedent::parse(&format!("{q}:{id}")).ok()
            })
            .collect::<Option<Vec<_>>>()?;
        Precedents::new(precedents, value.get("reason")?.as_str()?).ok()
    }
}

fn wait_key(wait: &Wait, index: usize) -> Option<(Option<&str>, Option<Vec<&str>>)> {
    match &wait.subject {
        Subject::Question(questions) => questions.get(index).map(|q| {
            (
                q.key.as_deref(),
                Some(
                    q.question
                        .options
                        .iter()
                        .map(|o| o.label.as_str())
                        .collect(),
                ),
            )
        }),
        Subject::Permission(p) if index == 0 => Some((p.key.as_deref(), None)),
        Subject::Permission(_) | Subject::Decision(_) | Subject::Input => None,
    }
}

fn kind_of(wait: &Wait) -> Option<EntryKind> {
    match wait.subject {
        Subject::Question(_) => Some(EntryKind::Question),
        Subject::Permission(_) => Some(EntryKind::Permission),
        Subject::Decision(_) => Some(EntryKind::Decision),
        Subject::Input => None,
    }
}

fn claim_key<'b>(
    book: &'b Book,
    kind: EntryKind,
    claim: &Claim,
    now: u64,
) -> Result<&'b str, String> {
    if claim.precedents.len() > MAX_PRECEDENTS {
        return Err(format!("선례가 {MAX_PRECEDENTS}개를 넘어요"));
    }
    let fp = answer_fp(claim.answer);
    let mut key: Option<&str> = None;
    for id in &claim.precedents {
        let entry = book
            .entry(id)
            .ok_or_else(|| format!("장부에 선례 `{id}` 가 없어요"))?;
        if entry.kind != kind {
            return Err(format!("선례 `{id}` 는 {} 결정이에요", entry.kind.id()));
        }
        if !(supports(entry) && in_window(entry, now)) {
            return Err(format!(
                "선례 `{id}` 는 사람이 선택지에서 직접 고른 90일 안의 결정이 아니에요"
            ));
        }
        let entry_key = entry
            .key
            .as_deref()
            .ok_or_else(|| format!("선례 `{id}` 에 결정 키가 없어요"))?;
        if key.is_some_and(|k| k != entry_key) {
            return Err("한 질문의 선례들이 서로 다른 결정이에요".to_string());
        }
        key = Some(entry_key);
        if entry.answer_fp != fp {
            return Err(format!("보낸 답이 선례 `{id}` 의 답과 달라요"));
        }
    }
    key.ok_or_else(|| "선례가 없어요".to_string())
}

/// Whether the answers may go on the precedents they name; the reason when
/// not. With the wait as the asking call holds it, the claims are checked
/// against it as well.
pub fn vouch(
    book: &Book,
    proxied: Proxied,
    claims: &[Claim],
    wait: Option<&Wait>,
    now: u64,
) -> Result<(), String> {
    let kind = proxied.kind();
    if claims.is_empty() {
        return Err("선례가 없어요".to_string());
    }
    if let Some(wait) = wait {
        if kind_of(wait) != Some(kind) {
            return Err("대기의 종류가 답과 달라요".to_string());
        }
        if !wait.proxyable() {
            let why: Vec<String> = wait.blocked.iter().map(|b| b.id()).collect();
            return Err(format!("대신 답할 수 없는 대기예요 ({})", why.join(", ")));
        }
    }
    for (index, claim) in claims.iter().enumerate() {
        if proxied == Proxied::Permission && claim.answer != Verdict::Allow.id() {
            return Err("권한 요청은 허용만 대신 답해요".to_string());
        }
        let key = claim_key(book, kind, claim, now)?;
        let key = DecisionKey::parse(key)?;
        let matched = judge(book, &key, now);
        if !matched.eligible {
            let why: Vec<&str> = matched.reasons.iter().map(|r| r.id()).collect();
            return Err(format!(
                "선례가 대리 조건을 채우지 않아요 ({})",
                why.join(", ")
            ));
        }
        if matched.mode != Mode::Act {
            return Err(format!("{} 은 아직 지켜보기 단계예요", kind.id()));
        }
        let Some(wait) = wait else {
            continue;
        };
        let (wait_key, labels) =
            wait_key(wait, index).ok_or_else(|| "답 수가 대기와 달라요".to_string())?;
        if wait_key != Some(key.as_str()) {
            return Err(format!("{}번 선례의 결정이 대기와 달라요", index + 1));
        }
        if labels.is_some_and(|labels| !labels.contains(&claim.answer)) {
            return Err(format!("{}번 답이 원래 선택지에 없어요", index + 1));
        }
    }
    if let Some(Subject::Question(questions)) = wait.map(|w| &w.subject) {
        if questions.len() != claims.len() {
            return Err("답 수가 질문 수와 달라요".to_string());
        }
    }
    Ok(())
}

/// Where the session that asked runs, as its mod hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlace {
    pub repo_root: Option<String>,
    pub cwd: String,
    pub permission_mode: Option<String>,
}

impl SessionPlace {
    pub fn place(&self) -> Place<'_> {
        Place {
            repo_root: self.repo_root.as_deref(),
            cwd: &self.cwd,
            permission_mode: self.permission_mode.as_deref(),
        }
    }
}

fn whole(on: WaitOn, place: &SessionPlace, paths: &dyn RealPaths) -> Wait {
    let waiting = Waiting {
        on,
        since: 0,
        request: None,
        context: None,
    };
    derive(&waiting, &place.place(), paths)
}

/// The wait of an AskUserQuestion call from its questions as the call holds them.
pub fn whole_question_wait(
    tool_use_id: &str,
    questions: &[Question],
    place: &SessionPlace,
    paths: &dyn RealPaths,
) -> Wait {
    let lengths = AskLengths {
        question: questions.iter().map(|q| js_len(&q.question)).collect(),
        labels: questions
            .iter()
            .map(|q| q.options.iter().map(|o| js_len(&o.label)).collect())
            .collect(),
    };
    let ask = Ask {
        tool_use_id: tool_use_id.to_string(),
        questions: questions.to_vec(),
        at: 0,
        orig_len: Some(lengths),
    };
    whole(WaitOn::Question(ask), place, paths)
}

/// The wait of a permission request from the tool's input as the request holds it.
pub fn whole_permission_wait(
    request_id: &str,
    tool: &str,
    input: &[(String, String)],
    place: &SessionPlace,
    paths: &dyn RealPaths,
) -> Wait {
    let ask = PermissionAsk {
        id: Some(request_id.to_string()),
        request: Activity {
            tool: tool.to_string(),
            input: input.to_vec(),
            at: 0,
        },
        input_len: Some(
            input
                .iter()
                .map(|(name, value)| (name.clone(), js_len(value)))
                .collect(),
        ),
    };
    whole(WaitOn::Permission(ask), place, paths)
}

/// The revoke of proxy answer `entry`, or why it cannot be revoked.
pub fn revoke(book: &Book, entry: &str, reason: &str, now: u64) -> Result<Revoke, String> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err("뒤집은 이유(--reason)가 비어 있어요".to_string());
    }
    if reason.chars().count() > MAX_REASON_CHARS {
        return Err(format!(
            "뒤집은 이유(--reason)가 {MAX_REASON_CHARS}자를 넘어요"
        ));
    }
    let found = book
        .entry(entry)
        .ok_or_else(|| format!("장부에 `{entry}` 가 없어요"))?;
    if found.decided_by != Some(DecidedBy::Proxy) {
        return Err(format!(
            "`{entry}` 는 대리 답이 아니에요 — 대리 답만 뒤집어요"
        ));
    }
    let key = found
        .key
        .clone()
        .ok_or_else(|| format!("`{entry}` 에는 결정 키가 없어요"))?;
    let suffix = digest(&[entry, &now.to_string(), reason], ID_BYTES);
    Ok(Revoke {
        v: VERSION,
        tag: RevokeTag::Revoke,
        id: format!("R{now}-{suffix}"),
        at: now,
        entry: entry.to_string(),
        key,
        kind: found.kind,
        reason: cut(&mask(reason), MAX_REASON_CHARS),
    })
}
