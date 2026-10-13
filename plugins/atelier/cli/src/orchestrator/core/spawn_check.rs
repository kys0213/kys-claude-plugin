//! Deterministic `agent.spawn` dispatch-rule checks. Every rule only applies
//! to non-fork dispatches — a fork inherits the parent process wholesale and
//! never picks a model or an effort at all, so none of "model unspecified",
//! "tier cap exceeded" or "effort unspecified" can be judged.

use super::tier::Tier;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpawnFacts {
    /// The model the dispatch site passed, if any (alias or full id).
    pub model: Option<String>,
    /// The model Claude Code actually resolved the dispatch to.
    pub resolved_model: Option<String>,
    /// The dispatching (parent) session's effective model id.
    pub parent_model: String,
    pub fork: bool,
    pub subagent_type: String,
    pub effort: EffortFact,
}

/// What the dispatch said about its reasoning effort.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EffortFact {
    /// The payload carried no effort at all (an adapter that predates the
    /// field), so whether the dispatch set one cannot be judged.
    #[default]
    Unknown,
    Unspecified,
    Given(String),
}

fn is_unspecified(model: &Option<String>) -> bool {
    match model {
        None => true,
        Some(m) => {
            let trimmed = m.trim();
            trimmed.is_empty() || trimmed.eq_ignore_ascii_case("inherit")
        }
    }
}

fn normalized(model: &str) -> String {
    model.trim().to_lowercase()
}

fn resolved(facts: &SpawnFacts) -> Option<&str> {
    facts
        .resolved_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn rule_inherited_model(facts: &SpawnFacts) -> Option<String> {
    if facts.fork || !is_unspecified(&facts.model) {
        return None;
    }
    let inherited = match resolved(facts) {
        None => format!("부모 model({}) 을 상속해", facts.parent_model.trim()),
        Some(r) if normalized(r) == normalized(&facts.parent_model) => {
            format!("부모 model 을 상속해 {r} 로")
        }
        Some(_) => return None,
    };
    Some(format!(
        "[atelier] 이 sub-agent({})는 {} 실행됐습니다. \
         이미 실행된 호출이라 다시 실행할 필요는 없고, 다음 dispatch 부터 작업에 맞는 model 을 명시하세요.",
        facts.subagent_type, inherited
    ))
}

fn rule_tier_cap_exceeded(facts: &SpawnFacts) -> Option<String> {
    if facts.fork {
        return None;
    }
    let requested = !is_unspecified(&facts.model);
    let effective = if requested {
        facts.model.as_deref()?.trim()
    } else {
        resolved(facts)?
    };
    let effective_tier = Tier::detect(effective)?;
    let cap = Tier::detect(&facts.parent_model)?.execution_cap();
    if effective_tier <= cap {
        return None;
    }
    let cause = if requested {
        "위임했습니다"
    } else if normalized(effective) == normalized(&facts.parent_model) {
        "상속돼 실행됐습니다"
    } else {
        "실행됐습니다"
    };
    Some(format!(
        "[atelier] 메인 {parent} 의 집행 위임 상한은 `{cap}` 인데 이 sub-agent 는 {effective} 로 {cause}. \
         이미 실행된 호출이라 다시 실행할 필요는 없습니다. 자문 소집이라면 무시해도 되고, \
         아니라면 다음 dispatch 부터 `{cap}` 이하로 지정하세요.",
        parent = facts.parent_model,
        cap = cap.alias(),
    ))
}

fn rule_effort_unspecified(facts: &SpawnFacts) -> Option<String> {
    if facts.fork || facts.effort != EffortFact::Unspecified {
        return None;
    }
    Some(format!(
        "[atelier] 이 sub-agent({})는 effort 를 지정하지 않아 기본 effort 로 실행됐습니다. \
         이미 실행된 호출이라 다시 실행할 필요는 없고, 다음 dispatch 부터 작업에 맞는 effort 를 명시하세요.",
        facts.subagent_type
    ))
}

pub fn check(facts: &SpawnFacts) -> Vec<String> {
    [
        rule_inherited_model(facts),
        rule_tier_cap_exceeded(facts),
        rule_effort_unspecified(facts),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> SpawnFacts {
        SpawnFacts {
            model: None,
            resolved_model: None,
            parent_model: "claude-opus-5-5".to_string(),
            fork: false,
            subagent_type: "general-purpose".to_string(),
            effort: EffortFact::Given("medium".to_string()),
        }
    }

    #[test]
    fn warns_when_effort_unspecified() {
        let mut f = facts();
        f.model = Some("sonnet".to_string());
        f.effort = EffortFact::Unspecified;
        let warnings = check(&f);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("effort"));
    }

    #[test]
    fn silent_when_effort_unknown() {
        let mut f = facts();
        f.model = Some("sonnet".to_string());
        f.effort = EffortFact::Unknown;
        assert!(check(&f).is_empty());
    }

    #[test]
    fn warns_when_model_unspecified_and_not_fork() {
        let warnings = check(&facts());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("상속해"));
    }

    #[test]
    fn silent_when_fork() {
        let mut f = facts();
        f.fork = true;
        assert!(check(&f).is_empty());
    }

    #[test]
    fn silent_when_model_explicit_and_within_cap() {
        let mut f = facts();
        f.model = Some("claude-opus-5-5".to_string());
        assert!(check(&f).is_empty());
    }

    #[test]
    fn warns_when_tier_cap_exceeded() {
        let mut f = facts();
        f.parent_model = "claude-sonnet-5".to_string();
        f.model = Some("claude-opus-5-5".to_string());
        let warnings = check(&f);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("집행 위임 상한"));
    }

    #[test]
    fn silent_when_tier_undetectable() {
        let mut f = facts();
        f.model = Some("gpt-5".to_string());
        f.parent_model = "gpt-5".to_string();
        assert!(check(&f).is_empty());
    }
}
