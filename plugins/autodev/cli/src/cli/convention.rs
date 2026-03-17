use std::path::Path;

use anyhow::Result;

use crate::core::repository::FeedbackPatternRepository;
use crate::infra::db::Database;

/// Detected technology stack from a repository.
#[derive(Debug, Default, Clone)]
pub struct TechStack {
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub databases: Vec<String>,
    pub test_tools: Vec<String>,
    pub build_tools: Vec<String>,
}

/// A convention file to be generated in `.claude/rules/`.
#[derive(Debug, Clone)]
pub struct ConventionFile {
    pub path: String,
    pub content: String,
    pub category: String,
}

/// Detect the technology stack by scanning for known marker files.
pub fn detect_tech_stack(repo_path: &Path) -> TechStack {
    let mut stack = TechStack::default();

    // Rust
    let cargo_toml = repo_path.join("Cargo.toml");
    if cargo_toml.exists() {
        stack.languages.push("Rust".to_string());
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            detect_rust_deps(&content, &mut stack);
        }
    }

    // TypeScript / JavaScript
    let package_json = repo_path.join("package.json");
    if package_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&package_json) {
            if content.contains("typescript") || content.contains("\"ts-") {
                stack.languages.push("TypeScript".to_string());
            } else {
                stack.languages.push("JavaScript".to_string());
            }
            detect_node_deps(&content, &mut stack);
        } else {
            stack.languages.push("JavaScript".to_string());
        }
    }

    // Go
    if repo_path.join("go.mod").exists() {
        stack.languages.push("Go".to_string());
    }

    // Python
    if repo_path.join("pyproject.toml").exists() || repo_path.join("requirements.txt").exists() {
        stack.languages.push("Python".to_string());
    }

    // Docker compose — detect databases
    for name in &[
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ] {
        let compose_path = repo_path.join(name);
        if compose_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&compose_path) {
                detect_compose_services(&content, &mut stack);
            }
            break;
        }
    }

    // CI
    if repo_path.join(".github/workflows").is_dir() {
        stack.build_tools.push("GitHub Actions".to_string());
    }

    // Build tools
    if repo_path.join("Makefile").exists() {
        stack.build_tools.push("Make".to_string());
    }
    if repo_path.join("Justfile").exists() || repo_path.join("justfile").exists() {
        stack.build_tools.push("Just".to_string());
    }

    stack
}

fn detect_rust_deps(content: &str, stack: &mut TechStack) {
    let frameworks = [
        ("axum", "Axum"),
        ("actix-web", "Actix Web"),
        ("rocket", "Rocket"),
        ("warp", "Warp"),
    ];
    for (dep, name) in &frameworks {
        if content.contains(dep) {
            stack.frameworks.push(name.to_string());
        }
    }

    if content.contains("tokio") {
        stack.frameworks.push("Tokio".to_string());
    }

    // Test tools — cargo test is always available for Rust
    stack.test_tools.push("cargo test".to_string());
}

fn detect_node_deps(content: &str, stack: &mut TechStack) {
    let frameworks = [
        ("\"react\"", "React"),
        ("\"next\"", "Next.js"),
        ("\"express\"", "Express"),
        ("\"fastify\"", "Fastify"),
        ("\"nestjs\"", "NestJS"),
        ("\"@nestjs/core\"", "NestJS"),
        ("\"vue\"", "Vue"),
        ("\"nuxt\"", "Nuxt"),
        ("\"svelte\"", "Svelte"),
    ];
    for (dep, name) in &frameworks {
        if content.contains(dep) && !stack.frameworks.contains(&name.to_string()) {
            stack.frameworks.push(name.to_string());
        }
    }

    let test_tools = [
        ("\"vitest\"", "Vitest"),
        ("\"jest\"", "Jest"),
        ("\"mocha\"", "Mocha"),
        ("\"playwright\"", "Playwright"),
        ("\"cypress\"", "Cypress"),
    ];
    for (dep, name) in &test_tools {
        if content.contains(dep) {
            stack.test_tools.push(name.to_string());
        }
    }

    let build_tools_list = [
        ("\"vite\"", "Vite"),
        ("\"webpack\"", "Webpack"),
        ("\"esbuild\"", "esbuild"),
        ("\"turbo\"", "Turborepo"),
    ];
    for (dep, name) in &build_tools_list {
        if content.contains(dep) {
            stack.build_tools.push(name.to_string());
        }
    }
}

fn detect_compose_services(content: &str, stack: &mut TechStack) {
    let databases = [
        ("postgres", "PostgreSQL"),
        ("mysql", "MySQL"),
        ("mariadb", "MariaDB"),
        ("mongo", "MongoDB"),
        ("redis", "Redis"),
        ("elasticsearch", "Elasticsearch"),
    ];
    for (marker, name) in &databases {
        if content.contains(marker) {
            stack.databases.push(name.to_string());
        }
    }
}

/// Generate convention files based on the detected tech stack.
pub fn generate_conventions(stack: &TechStack) -> Vec<ConventionFile> {
    let mut files = Vec::new();

    // Always include common conventions
    files.push(ConventionFile {
        path: ".claude/rules/git-workflow.md".to_string(),
        content: COMMON_GIT_WORKFLOW.to_string(),
        category: "common".to_string(),
    });

    files.push(ConventionFile {
        path: ".claude/rules/code-review.md".to_string(),
        content: COMMON_CODE_REVIEW.to_string(),
        category: "common".to_string(),
    });

    // Rust conventions
    if stack.languages.contains(&"Rust".to_string()) {
        files.push(ConventionFile {
            path: ".claude/rules/rust-project-structure.md".to_string(),
            content: RUST_PROJECT_STRUCTURE.to_string(),
            category: "project-structure".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/rust-error-handling.md".to_string(),
            content: RUST_ERROR_HANDLING.to_string(),
            category: "error-handling".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/rust-testing.md".to_string(),
            content: RUST_TESTING.to_string(),
            category: "testing".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/rust-clippy.md".to_string(),
            content: RUST_CLIPPY.to_string(),
            category: "linting".to_string(),
        });
    }

    // TypeScript conventions
    if stack.languages.contains(&"TypeScript".to_string()) {
        files.push(ConventionFile {
            path: ".claude/rules/typescript-project-structure.md".to_string(),
            content: TS_PROJECT_STRUCTURE.to_string(),
            category: "project-structure".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/typescript-type-strategy.md".to_string(),
            content: TS_TYPE_STRATEGY.to_string(),
            category: "type-strategy".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/typescript-testing.md".to_string(),
            content: TS_TESTING.to_string(),
            category: "testing".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/typescript-linting.md".to_string(),
            content: TS_LINTING.to_string(),
            category: "linting".to_string(),
        });
    }

    files
}

/// Result of a bootstrap operation.
#[derive(Debug)]
pub struct BootstrapResult {
    pub files_written: Vec<String>,
    pub files_skipped: Vec<String>,
}

/// Bootstrap convention files into the target repository.
///
/// If `apply` is false, performs a dry-run and returns what would be written.
/// If `apply` is true, writes convention files to disk (does not overwrite existing).
pub fn bootstrap(
    repo_path: &Path,
    stack: &TechStack,
    apply: bool,
) -> anyhow::Result<BootstrapResult> {
    let conventions = generate_conventions(stack);
    let mut result = BootstrapResult {
        files_written: Vec::new(),
        files_skipped: Vec::new(),
    };

    for conv in &conventions {
        let target = repo_path.join(&conv.path);

        if target.exists() {
            result.files_skipped.push(conv.path.clone());
            continue;
        }

        if apply {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, &conv.content)?;
        }

        result.files_written.push(conv.path.clone());
    }

    Ok(result)
}

/// Format the detected tech stack for display.
pub fn format_tech_stack(stack: &TechStack) -> String {
    let mut output = String::new();

    output.push_str("Detected Tech Stack:\n\n");

    if stack.languages.is_empty()
        && stack.frameworks.is_empty()
        && stack.databases.is_empty()
        && stack.test_tools.is_empty()
        && stack.build_tools.is_empty()
    {
        output.push_str("  (no technology stack detected)\n");
        return output;
    }

    if !stack.languages.is_empty() {
        output.push_str(&format!("  Languages:   {}\n", stack.languages.join(", ")));
    }
    if !stack.frameworks.is_empty() {
        output.push_str(&format!("  Frameworks:  {}\n", stack.frameworks.join(", ")));
    }
    if !stack.databases.is_empty() {
        output.push_str(&format!("  Databases:   {}\n", stack.databases.join(", ")));
    }
    if !stack.test_tools.is_empty() {
        output.push_str(&format!("  Test tools:  {}\n", stack.test_tools.join(", ")));
    }
    if !stack.build_tools.is_empty() {
        output.push_str(&format!(
            "  Build tools: {}\n",
            stack.build_tools.join(", ")
        ));
    }

    output
}

/// Format the bootstrap result for display.
pub fn format_bootstrap_result(result: &BootstrapResult, dry_run: bool) -> String {
    let mut output = String::new();

    if dry_run {
        output.push_str("Dry-run: the following files would be created:\n\n");
    } else {
        output.push_str("Bootstrap complete:\n\n");
    }

    for f in &result.files_written {
        let prefix = if dry_run {
            "  [would create]"
        } else {
            "  [created]"
        };
        output.push_str(&format!("{prefix} {f}\n"));
    }

    for f in &result.files_skipped {
        output.push_str(&format!("  [skipped — already exists] {f}\n"));
    }

    if result.files_written.is_empty() && result.files_skipped.is_empty() {
        output.push_str("  (no convention files to generate)\n");
    }

    output
}

// ─── Convention templates ───

const COMMON_GIT_WORKFLOW: &str = r#"# Git Workflow

## Commit Messages
Use conventional commits format: `<type>(<scope>): <description>`

Types: feat, fix, refactor, docs, test, chore, ci, perf

## Branch Naming
```
<type>/<short-description>
```

## Pull Requests
- Keep PRs focused on a single change
- Write clear descriptions with context
- Link related issues
- Ensure CI passes before requesting review
"#;

const COMMON_CODE_REVIEW: &str = r#"# Code Review Policy

## Review Checklist
1. All tests pass
2. No new warnings from linting/formatting tools
3. Changes match the stated purpose (no scope creep)
4. Error handling is appropriate
5. No hardcoded secrets or credentials

## Review Flow
- Author self-reviews before requesting review
- At least one approval required before merge
- Address all review comments before merging
"#;

const RUST_PROJECT_STRUCTURE: &str = r#"# Rust Project Structure

## Layout
```
src/
  lib.rs          # Library root — re-exports public modules
  main.rs         # Binary entry point
  core/           # Domain logic (no external dependencies)
  infra/          # External system adapters (DB, HTTP, CLI)
  cli/            # CLI command handlers
tests/            # Integration tests
```

## Principles
- `core/` must not depend on `infra/` — use traits for abstraction
- Keep `main.rs` thin — delegate to library code
- Use `mod.rs` to organize module trees
- Prefer `lib.rs` re-exports for public API
"#;

const RUST_ERROR_HANDLING: &str = r#"# Rust Error Handling

## Strategy
- Use `anyhow::Result` for application-level errors (CLI, main)
- Use `thiserror` for library-level error types with structured variants
- Avoid `.unwrap()` in production code — use `?` operator

## Pattern
```rust
// Library errors
#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
}

// Application code
fn run() -> anyhow::Result<()> {
    let value = find_item(id).context("failed to find item")?;
    Ok(())
}
```

## Context
Always add context to errors using `.context()` or `.with_context()` so that
error messages form a readable chain.
"#;

const RUST_TESTING: &str = r#"# Rust Testing

## Test Organization
- Unit tests: `#[cfg(test)] mod tests` inside each module
- Integration tests: `tests/` directory at crate root

## Conventions
- Use `tempfile::TempDir` for filesystem isolation
- Use trait-based mocks for external dependencies
- Test the public API, not internal implementation
- Name tests descriptively: `<function>_<scenario>_<expected>`

## Running
```bash
cargo test                    # all tests
cargo test --test <name>      # specific integration test
cargo test -- --nocapture     # show stdout
```
"#;

const RUST_CLIPPY: &str = r#"# Rust Clippy & Formatting

## Rules
- `cargo fmt --check` must pass — run `cargo fmt` to fix
- `cargo clippy -- -D warnings` must pass
- Fix clippy warnings in code; only use `#[allow]` as last resort with justification

## Common Clippy Fixes
- `clippy::needless_return` — remove explicit `return` at end of function
- `clippy::redundant_clone` — remove unnecessary `.clone()`
- `clippy::single_match` — convert single-arm `match` to `if let`
"#;

const TS_PROJECT_STRUCTURE: &str = r#"# TypeScript Project Structure

## Layout
```
src/
  index.ts        # Entry point
  types/          # Shared type definitions
  lib/            # Core business logic
  api/            # API routes/handlers
  utils/          # Utility functions
tests/            # Test files (mirror src/ structure)
```

## Principles
- Co-locate types with the modules that use them
- Use barrel exports (index.ts) for public API
- Keep dependency injection at the boundary
- Separate pure logic from side-effect code
"#;

const TS_TYPE_STRATEGY: &str = r#"# TypeScript Type Strategy

## Rules
- Enable `strict: true` in tsconfig.json
- Avoid `any` — use `unknown` when type is truly unknown
- Prefer `interface` for object shapes, `type` for unions/intersections
- Use branded types for domain identifiers

## Patterns
```typescript
// Branded type for type-safe IDs
type UserId = string & { readonly __brand: 'UserId' };

// Discriminated union for states
type Result<T> =
  | { success: true; data: T }
  | { success: false; error: string };

// Use `satisfies` for type checking without widening
const config = { ... } satisfies Config;
```

## Avoid
- `as` type assertions (use type guards instead)
- Non-null assertions `!` (handle null explicitly)
- `@ts-ignore` (fix the type error instead)
"#;

const TS_TESTING: &str = r#"# TypeScript Testing

## Framework
Use Vitest (or Jest) for unit/integration tests.

## Conventions
- Test files: `*.test.ts` or `*.spec.ts`
- Use descriptive test names: `it('should <behavior> when <condition>')`
- Mock external dependencies, not internal modules
- Test behavior, not implementation

## Running
```bash
npx vitest          # watch mode
npx vitest run      # single run
npx vitest --coverage
```
"#;

const TS_LINTING: &str = r#"# TypeScript Linting

## Tools
- ESLint with TypeScript plugin
- Prettier for formatting

## Rules
- No unused variables or imports
- Consistent import ordering
- No console.log in production code (use a logger)

## Running
```bash
npx eslint .
npx prettier --check .
```
"#;

// ─── Feedback Patterns CLI ───

/// List feedback patterns for a repo, formatted as a table.
pub fn patterns(
    db: &Database,
    repo_id: Option<&str>,
    min_count: Option<i32>,
    json: bool,
) -> Result<String> {
    let patterns = if let Some(rid) = repo_id {
        if let Some(mc) = min_count.filter(|&c| c > 1) {
            db.feedback_list_actionable(rid, mc)?
        } else {
            db.feedback_list(rid)?
        }
    } else {
        // No repo_id: return empty with a message
        return Ok("Specify --repo to list feedback patterns.\n".to_string());
    };

    if json {
        return Ok(serde_json::to_string_pretty(&patterns)?);
    }

    if patterns.is_empty() {
        return Ok("No feedback patterns found.\n".to_string());
    }

    let mut output = String::new();
    output.push_str(&format!(
        "{:<8} {:<16} {:<10} {:<8} {:<10} {}\n",
        "COUNT", "TYPE", "STATUS", "CONF", "SOURCE", "SUGGESTION"
    ));
    output.push_str(&format!("{}\n", "-".repeat(80)));

    for p in &patterns {
        let suggestion_short = if p.suggestion.len() > 40 {
            format!("{}...", &p.suggestion[..37])
        } else {
            p.suggestion.clone()
        };
        output.push_str(&format!(
            "{:<8} {:<16} {:<10} {:<8.2} {:<10} {}\n",
            p.occurrence_count,
            p.pattern_type,
            p.status.as_str(),
            p.confidence,
            p.source,
            suggestion_short,
        ));
    }

    Ok(output)
}
