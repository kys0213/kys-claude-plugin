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

pub struct ClassifiedTool {
    pub classified_name: String,
}

/// Classify a tool usage entry.
/// Bash commands are sub-classified into git/test/build/lint/other.
pub fn classify_tool(tool_name: &str, input: Option<&serde_json::Value>) -> ClassifiedTool {
    if tool_name != "Bash" {
        return ClassifiedTool {
            classified_name: tool_name.to_string(),
        };
    }

    let command = input
        .and_then(|v| v.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let classified_name = if GIT_PATTERN.is_match(command) {
        "Bash:git"
    } else if TEST_PATTERN.is_match(command) {
        "Bash:test"
    } else if BUILD_PATTERN.is_match(command) {
        "Bash:build"
    } else if LINT_PATTERN.is_match(command) {
        "Bash:lint"
    } else {
        "Bash:other"
    };

    ClassifiedTool {
        classified_name: classified_name.to_string(),
    }
}
