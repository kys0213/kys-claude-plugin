//! Deterministic `agent.spawn` dispatch-rule checks. Both rules only apply
//! to non-fork dispatches — a fork inherits the parent process wholesale and
//! never picks a model at all, so neither "model unspecified" nor "tier cap
//! exceeded" can be judged.

use super::tier::Tier;

/// Facts about one `agent.spawn` dispatch, already lenient-defaulted by the
/// caller (`commands::payload::parse_spawn_facts`) — this module only
/// interprets them, it never guesses at a missing field.
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

fn rule_unspecified_model(facts: &SpawnFacts) -> Option<String> {
    if facts.fork || !is_unspecified(&facts.model) {
        return None;
    }
    let effective = facts
        .resolved_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("부모 model");
    Some(format!(
        "[atelier] model 을 지정하지 않은 dispatch 입니다 — {} 가 {} 로 실행됩니다 (부모: {}). \
         orchestrator 불변식 21: 매 dispatch 에 표 1 의 시작 tier 로 model 을 명시하세요.",
        facts.subagent_type, effective, facts.parent_model
    ))
}

fn rule_tier_cap_exceeded(facts: &SpawnFacts) -> Option<String> {
    if facts.fork {
        return None;
    }
    let model = facts.model.as_ref()?;
    let model_tier = Tier::detect(model)?;
    let parent_tier = Tier::detect(&facts.parent_model)?;
    let cap = parent_tier.execution_cap();
    if model_tier > cap {
        Some(format!(
            "[atelier] 집행 위임 tier 상한 초과 — 메인 {} 의 상한은 {:?} 인데 {} 로 위임했습니다. \
             자문 소집이 아니라면 표 1 사전 기준의 상한 이하로 다시 지정하세요.",
            facts.parent_model, cap, model
        ))
    } else {
        None
    }
}

pub fn check(facts: &SpawnFacts) -> Vec<String> {
    [rule_unspecified_model(facts), rule_tier_cap_exceeded(facts)]
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
        }
    }

    #[test]
    fn warns_when_model_unspecified_and_not_fork() {
        let warnings = check(&facts());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("model 을 지정하지 않은"));
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
        assert!(warnings[0].contains("tier 상한 초과"));
    }

    #[test]
    fn silent_when_tier_undetectable() {
        let mut f = facts();
        f.model = Some("gpt-5".to_string());
        f.parent_model = "gpt-5".to_string();
        assert!(check(&f).is_empty());
    }
}
