use regex::Regex;
use std::sync::LazyLock;

static GIT_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^git\s|^gh\s").unwrap());
static TEST_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(vitest|jest|mocha|pytest|cargo\s+test|go\s+test|npm\s+test|bun\s+test)\b").unwrap()
});
static BUILD_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(tsc|webpack|vite\s+build|next\s+build|npm\s+run\s+build|cargo\s+build|go\s+build|make|cmake)\b").unwrap()
});
static LINT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(eslint|prettier|biome|rubocop|flake8|ruff|clippy|golangci-lint)\b").unwrap()
});

#[allow(dead_code)]
pub struct ClassifiedTool {
    pub original_name: String,
    pub classified_name: String,
    pub category: Option<String>,
}

/// Classify a tool usage entry
pub fn classify_tool(tool_name: &str, input: Option<&serde_json::Value>) -> ClassifiedTool {
    if tool_name != "Bash" {
        return ClassifiedTool {
            original_name: tool_name.to_string(),
            classified_name: tool_name.to_string(),
            category: None,
        };
    }

    let command = input
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if GIT_PATTERN.is_match(command) {
        return ClassifiedTool {
            original_name: tool_name.to_string(),
            classified_name: "Bash:git".to_string(),
            category: Some("git".to_string()),
        };
    }

    if TEST_PATTERN.is_match(command) {
        return ClassifiedTool {
            original_name: tool_name.to_string(),
            classified_name: "Bash:test".to_string(),
            category: Some("test".to_string()),
        };
    }

    if BUILD_PATTERN.is_match(command) {
        return ClassifiedTool {
            original_name: tool_name.to_string(),
            classified_name: "Bash:build".to_string(),
            category: Some("build".to_string()),
        };
    }

    if LINT_PATTERN.is_match(command) {
        return ClassifiedTool {
            original_name: tool_name.to_string(),
            classified_name: "Bash:lint".to_string(),
            category: Some("lint".to_string()),
        };
    }

    ClassifiedTool {
        original_name: tool_name.to_string(),
        classified_name: "Bash:other".to_string(),
        category: Some("other".to_string()),
    }
}
