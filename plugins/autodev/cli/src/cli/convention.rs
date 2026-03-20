use std::path::Path;

use anyhow::Result;

use crate::core::models::FeedbackPatternStatus;
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
    let go_mod = repo_path.join("go.mod");
    if go_mod.exists() {
        stack.languages.push("Go".to_string());
        if let Ok(content) = std::fs::read_to_string(&go_mod) {
            detect_go_deps(&content, &mut stack);
        }
    }

    // Python
    if repo_path.join("pyproject.toml").exists() || repo_path.join("requirements.txt").exists() {
        stack.languages.push("Python".to_string());
        detect_python_deps(repo_path, &mut stack);
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

fn detect_go_deps(content: &str, stack: &mut TechStack) {
    let frameworks = [
        ("github.com/gin-gonic/gin", "Gin"),
        ("github.com/labstack/echo", "Echo"),
        ("github.com/go-chi/chi", "Chi"),
        ("github.com/gofiber/fiber", "Fiber"),
        ("github.com/gorilla/mux", "Gorilla Mux"),
        ("google.golang.org/grpc", "gRPC"),
    ];
    for (dep, name) in &frameworks {
        if content.contains(dep) {
            stack.frameworks.push(name.to_string());
        }
    }

    // go test is always available
    stack.test_tools.push("go test".to_string());
}

fn detect_python_deps(repo_path: &Path, stack: &mut TechStack) {
    // Check pyproject.toml and requirements.txt for frameworks
    let sources = [
        repo_path.join("pyproject.toml"),
        repo_path.join("requirements.txt"),
    ];
    for source in &sources {
        if let Ok(content) = std::fs::read_to_string(source) {
            let frameworks = [
                ("fastapi", "FastAPI"),
                ("django", "Django"),
                ("flask", "Flask"),
                ("starlette", "Starlette"),
            ];
            for (dep, name) in &frameworks {
                if content.contains(dep) && !stack.frameworks.contains(&name.to_string()) {
                    stack.frameworks.push(name.to_string());
                }
            }

            let test_tools = [
                ("pytest", "pytest"),
                ("unittest", "unittest"),
                ("mypy", "mypy"),
            ];
            for (dep, name) in &test_tools {
                if content.contains(dep) && !stack.test_tools.contains(&name.to_string()) {
                    stack.test_tools.push(name.to_string());
                }
            }
        }
    }

    // pytest is the de facto standard
    if !stack.test_tools.iter().any(|t| t == "pytest") {
        stack.test_tools.push("pytest".to_string());
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

    // Go conventions
    if stack.languages.contains(&"Go".to_string()) {
        files.push(ConventionFile {
            path: ".claude/rules/go-project-structure.md".to_string(),
            content: GO_PROJECT_STRUCTURE.to_string(),
            category: "project-structure".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/go-error-handling.md".to_string(),
            content: GO_ERROR_HANDLING.to_string(),
            category: "error-handling".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/go-testing.md".to_string(),
            content: GO_TESTING.to_string(),
            category: "testing".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/go-linting.md".to_string(),
            content: GO_LINTING.to_string(),
            category: "linting".to_string(),
        });
    }

    // Python conventions
    if stack.languages.contains(&"Python".to_string()) {
        files.push(ConventionFile {
            path: ".claude/rules/python-project-structure.md".to_string(),
            content: PY_PROJECT_STRUCTURE.to_string(),
            category: "project-structure".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/python-type-hints.md".to_string(),
            content: PY_TYPE_HINTS.to_string(),
            category: "type-strategy".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/python-testing.md".to_string(),
            content: PY_TESTING.to_string(),
            category: "testing".to_string(),
        });

        files.push(ConventionFile {
            path: ".claude/rules/python-linting.md".to_string(),
            content: PY_LINTING.to_string(),
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

// ─── Go convention templates ───

const GO_PROJECT_STRUCTURE: &str = r#"# Go Project Structure

## Layout
- `cmd/` — application entry points
- `internal/` — private application code (not importable)
- `pkg/` — public library code (importable)
- `api/` — API definitions (proto, OpenAPI specs)

## Principles
- Keep `main.go` minimal — wire dependencies and start the server
- Use `internal/` for domain logic to prevent external imports
- Group by domain, not by layer (e.g., `internal/user/` not `internal/handlers/`)
"#;

const GO_ERROR_HANDLING: &str = r#"# Go Error Handling

## Strategy
- Always check returned errors — never use `_` for error values
- Wrap errors with context using `fmt.Errorf("operation: %w", err)`
- Use sentinel errors (`var ErrNotFound = errors.New(...)`) for expected conditions
- Use custom error types for errors that carry structured data

## Pattern
```go
if err != nil {
    return fmt.Errorf("fetching user %d: %w", id, err)
}
```

## Anti-patterns
- Do not `log.Fatal` in library code — return errors to the caller
- Do not use `panic` for expected error conditions
"#;

const GO_TESTING: &str = r#"# Go Testing

## Organization
- Test files: `*_test.go` in the same package
- Table-driven tests for multiple cases
- Use `testify/assert` or standard library `testing`
- Integration tests: use build tag `//go:build integration`

## Running
```bash
go test ./...
go test -race ./...
go test -v -run TestSpecificName ./pkg/...
```

## Conventions
- Test function names: `TestFunctionName_Scenario`
- Use `t.Helper()` in test helper functions
- Use `t.Parallel()` for independent tests
"#;

const GO_LINTING: &str = r#"# Go Linting & Formatting

## Tools
- `gofmt` / `goimports` for formatting (non-negotiable)
- `golangci-lint` for comprehensive linting
- `go vet` for suspicious constructs

## Running
```bash
gofmt -l .
golangci-lint run ./...
go vet ./...
```

## Rules
- All code must be gofmt'd
- No unused imports or variables
- No shadowed variables in critical paths
"#;

// ─── Python convention templates ───

const PY_PROJECT_STRUCTURE: &str = r#"# Python Project Structure

## Layout
- `src/<package>/` — source code (src-layout recommended)
- `tests/` — test files mirroring source structure
- `pyproject.toml` — project metadata and dependencies

## Principles
- Use `pyproject.toml` over `setup.py` for new projects
- Virtual environments: use `venv`, `poetry`, or `uv`
- Keep `__init__.py` files minimal — avoid heavy imports
- Group by domain module, not by layer
"#;

const PY_TYPE_HINTS: &str = r#"# Python Type Hints

## Rules
- Add type hints to all public function signatures
- Use `from __future__ import annotations` for modern syntax
- Use `Optional[T]` or `T | None` (3.10+) for nullable values
- Use `Protocol` for structural typing (duck typing with safety)

## Patterns
```python
def get_user(user_id: int) -> User | None:
    ...

class Repository(Protocol):
    def find(self, id: str) -> Entity | None: ...
```

## Tools
- `mypy --strict` for type checking
- `pyright` as an alternative type checker
"#;

const PY_TESTING: &str = r#"# Python Testing

## Framework
- pytest as the standard test framework
- Use fixtures for setup/teardown
- Use `conftest.py` for shared fixtures

## Organization
```
tests/
  conftest.py          # shared fixtures
  test_user_service.py # mirrors src/app/user_service.py
  integration/         # integration tests
```

## Running
```bash
pytest
pytest -x --tb=short
pytest -k "test_specific_name"
pytest --cov=src
```

## Conventions
- Test function names: `test_function_name_scenario`
- Use `@pytest.mark.parametrize` for table-driven tests
"#;

const PY_LINTING: &str = r#"# Python Linting & Formatting

## Tools
- `ruff` for linting and formatting (fast, all-in-one)
- `mypy` for type checking
- `black` or `ruff format` for code formatting

## Running
```bash
ruff check .
ruff format --check .
mypy src/
```

## Rules
- All code must pass ruff checks
- No unused imports or variables
- Consistent import ordering (stdlib → third-party → local)
- Line length: 88 characters (black default)
"#;

// ─── Feedback Collection from HITL ───

/// Classify the pattern type from a HITL event's situation text using keyword matching.
pub fn classify_pattern_type(situation: &str) -> &'static str {
    let lower = situation.to_lowercase();
    if lower.contains("style") || lower.contains("format") || lower.contains("lint") {
        "style"
    } else if lower.contains("test") {
        "testing"
    } else if lower.contains("error") || lower.contains("fail") {
        "error-handling"
    } else if lower.contains("review") || lower.contains("iteration") {
        "review-process"
    } else if lower.contains("conflict") || lower.contains("spec") {
        "spec-management"
    } else {
        "general"
    }
}

/// Build a feedback pattern from a HITL event's situation and a user message.
fn build_hitl_feedback(
    repo_id: &str,
    situation: &str,
    message: &str,
) -> crate::core::models::NewFeedbackPattern {
    crate::core::models::NewFeedbackPattern {
        repo_id: repo_id.to_string(),
        pattern_type: classify_pattern_type(situation).to_string(),
        suggestion: message.to_string(),
        source: "hitl".to_string(),
    }
}

/// Collect feedback patterns from responded HITL events for a given repo.
///
/// `repo_name` is the human-readable name (e.g. "org/repo") used to query HITL events.
/// `repo_id` is the internal UUID used for feedback pattern storage.
///
/// Uses `scan_cursors` with target `"feedback-hitl"` to track the last processed
/// event's `created_at` timestamp, ensuring each HITL response is only processed once.
pub fn collect_feedback(db: &Database, repo_name: &str, repo_id: &str) -> Result<String> {
    use crate::core::models::HitlStatus;
    use crate::core::repository::{HitlRepository, ScanCursorRepository};

    let cursor_target = "feedback-hitl";
    let last_seen = db.cursor_get_last_seen(repo_id, cursor_target)?;

    let events = db.hitl_list(Some(repo_name))?;
    let mut collected = 0;
    let mut total_responded = 0;
    let mut max_created_at: Option<String> = None;

    for event in &events {
        if !matches!(event.status, HitlStatus::Responded) {
            continue;
        }

        // Skip events already processed in a previous run
        if let Some(ref cursor) = last_seen {
            if event.created_at <= *cursor {
                continue;
            }
        }

        total_responded += 1;

        // Track the latest event timestamp for cursor update
        match max_created_at {
            None => max_created_at = Some(event.created_at.clone()),
            Some(ref current) if event.created_at > *current => {
                max_created_at = Some(event.created_at.clone());
            }
            _ => {}
        }

        let responses = db.hitl_responses(&event.id)?;
        for resp in &responses {
            if let Some(ref message) = resp.message {
                if message.trim().is_empty() {
                    continue;
                }
                db.feedback_upsert(&build_hitl_feedback(repo_id, &event.situation, message))?;
                collected += 1;
            }
        }
    }

    // Persist cursor so next run skips these events
    if let Some(ref ts) = max_created_at {
        db.cursor_upsert(repo_id, cursor_target, ts)?;
    }

    Ok(format!(
        "Collected {collected} feedback pattern(s) from {total_responded} HITL responses\n"
    ))
}

/// Collect feedback from a single HITL response (used for auto-collection after respond).
///
/// Accepts repo_id and situation directly to avoid re-querying the HITL event.
pub fn collect_feedback_from_hitl(
    db: &Database,
    repo_id: &str,
    situation: &str,
    message: &str,
) -> Result<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    db.feedback_upsert(&build_hitl_feedback(repo_id, situation, message))?;
    Ok(())
}

// ─── Feedback Collection from PR Reviews ───

/// A single PR review comment extracted from the GitHub API response.
#[derive(Debug)]
struct PrReviewComment {
    pub body: String,
}

/// Parse PR review comments from the raw JSON returned by `gh api --paginate`.
///
/// The GitHub API returns an array of review objects. Each review has a `body`
/// field (the top-level review comment). We extract non-empty body fields.
fn parse_review_comments(raw_json: &[u8]) -> Vec<PrReviewComment> {
    let mut comments = Vec::new();

    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(raw_json) else {
        return comments;
    };

    // Handle both a single JSON array and concatenated arrays (from --paginate)
    let items = match &parsed {
        serde_json::Value::Array(arr) => arr.clone(),
        _ => vec![parsed],
    };

    for item in &items {
        if let Some(body) = item.get("body").and_then(|b| b.as_str()) {
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                comments.push(PrReviewComment {
                    body: trimmed.to_string(),
                });
            }
        }
    }

    comments
}

/// Classify a PR review comment body into a pattern type.
///
/// Extends `classify_pattern_type` with additional keywords commonly found
/// in PR review comments (e.g., naming, documentation).
pub fn classify_review_comment(body: &str) -> &'static str {
    let lower = body.to_lowercase();

    // Naming conventions
    if lower.contains("naming")
        || lower.contains("rename")
        || lower.contains("variable name")
        || lower.contains("snake_case")
        || lower.contains("camelcase")
    {
        return "style";
    }

    // Documentation
    if lower.contains("document")
        || lower.contains("rustdoc")
        || lower.contains("jsdoc")
        || lower.contains("docstring")
    {
        return "style";
    }

    // Fall back to the existing classifier
    classify_pattern_type(body)
}

/// Group review comments by their classified pattern type.
/// Comments with the same pattern type are deduplicated by content.
fn group_comments_by_theme(
    comments: &[PrReviewComment],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for comment in comments {
        let pattern_type = classify_review_comment(&comment.body).to_string();
        let suggestions = groups.entry(pattern_type).or_default();

        // Deduplicate: skip if an identical suggestion already exists
        if !suggestions.iter().any(|s| s == &comment.body) {
            suggestions.push(comment.body.clone());
        }
    }

    groups
}

/// Collect feedback patterns from PR review comments for a given repo.
///
/// Fetches recently closed PRs, then fetches review comments for each PR.
/// Groups comments by theme/pattern and stores detected patterns in the
/// `feedback_patterns` table.
///
/// Uses cursor-based pagination via `api_paginate` and batches requests
/// to be mindful of API rate limits (max 30 recent PRs per run).
///
/// `repo_name` is the GitHub slug (e.g. "org/repo").
/// `repo_id` is the internal UUID for feedback pattern storage.
/// `gh_host` is an optional GitHub Enterprise hostname.
pub async fn collect_feedback_from_pr_reviews(
    db: &Database,
    gh: &dyn crate::infra::gh::Gh,
    repo_name: &str,
    repo_id: &str,
    gh_host: Option<&str>,
) -> Result<String> {
    // Fetch recently closed/merged PRs (limit to 30 to respect rate limits)
    let pulls_json = gh
        .api_paginate(
            repo_name,
            "pulls",
            &[
                ("state", "closed"),
                ("sort", "updated"),
                ("direction", "desc"),
                ("per_page", "30"),
            ],
            gh_host,
        )
        .await?;

    let pulls: Vec<serde_json::Value> = match serde_json::from_slice(&pulls_json) {
        Ok(serde_json::Value::Array(arr)) => arr,
        _ => return Ok("No closed PRs found.\n".to_string()),
    };

    let mut total_comments = 0u32;
    let mut total_patterns = 0u32;

    for pull in &pulls {
        let Some(pr_number) = pull.get("number").and_then(|n| n.as_i64()) else {
            continue;
        };

        // Fetch review comments for this PR
        let reviews_json = match gh
            .api_paginate(
                repo_name,
                &format!("pulls/{pr_number}/reviews"),
                &[("per_page", "100")],
                gh_host,
            )
            .await
        {
            Ok(data) => data,
            Err(_) => continue, // Skip on API error (rate limit, etc.)
        };

        let comments = parse_review_comments(&reviews_json);
        total_comments += comments.len() as u32;

        let groups = group_comments_by_theme(&comments);

        for (pattern_type, suggestions) in &groups {
            for suggestion in suggestions {
                let pattern = crate::core::models::NewFeedbackPattern {
                    repo_id: repo_id.to_string(),
                    pattern_type: pattern_type.clone(),
                    suggestion: suggestion.clone(),
                    source: "pr-review".to_string(),
                };
                db.feedback_upsert(&pattern)?;
                total_patterns += 1;
            }
        }
    }

    Ok(format!(
        "Collected {total_patterns} feedback pattern(s) from {total_comments} PR review comment(s) across {} PR(s)\n",
        pulls.len()
    ))
}

// ─── Convention Update Proposal ───

/// Map pattern_type to a convention rule file path.
pub fn pattern_type_to_rule_file(pattern_type: &str) -> String {
    format!(".claude/rules/{pattern_type}.md")
}

/// propose_updates의 결과: 출력 메시지와 생성된 HITL 이벤트 목록.
pub struct ProposeResult {
    pub output: String,
    /// Pairs of (HITL event, assigned hitl_id) for notification dispatch.
    pub hitl_entries: Vec<(crate::core::models::NewHitlEvent, String)>,
}

/// Check actionable feedback patterns and create HITL events for convention updates.
///
/// Queries patterns with `occurrence_count >= threshold` and `status = Active`,
/// then creates a HITL event for each so a human can approve, edit, or reject.
/// Returns a summary message and created HITL events for notification dispatch.
pub fn propose_updates(db: &Database, repo_id: &str, threshold: i32) -> Result<ProposeResult> {
    use crate::core::models::{HitlSeverity, NewHitlEvent};
    use crate::core::repository::HitlRepository;

    let patterns = db.feedback_list_actionable(repo_id, threshold)?;

    if patterns.is_empty() {
        return Ok(ProposeResult {
            output: "No actionable patterns found.\n".to_string(),
            hitl_entries: Vec::new(),
        });
    }

    let mut hitl_entries = Vec::new();

    for pattern in &patterns {
        let rule_file = pattern_type_to_rule_file(&pattern.pattern_type);

        let hitl_event = NewHitlEvent {
            repo_id: repo_id.to_string(),
            spec_id: None,
            work_id: None,
            severity: HitlSeverity::Medium,
            situation: format!("Convention update suggested: {}", pattern.pattern_type),
            context: format!(
                "Rule file: {}\nOccurrences: {}\nSuggestion: {}\nSources: {}",
                rule_file, pattern.occurrence_count, pattern.suggestion, pattern.sources_json
            ),
            options: vec![
                "Apply this convention rule".to_string(),
                "Edit and apply".to_string(),
                "Reject".to_string(),
            ],
        };

        let hitl_id = db.hitl_create(&hitl_event)?;
        db.feedback_set_status(&pattern.id, FeedbackPatternStatus::Proposed)?;
        hitl_entries.push((hitl_event, hitl_id));
    }

    Ok(ProposeResult {
        output: format!("Proposed {} convention update(s)\n", hitl_entries.len()),
        hitl_entries,
    })
}

// ─── Convention Apply ───

/// Parse the convention update context from a HITL event.
///
/// The context field has the format:
/// ```text
/// Rule file: .claude/rules/error-handling.md
/// Occurrences: 5
/// Suggestion: Use thiserror for all error types
/// Sources: {"hitl": 3, "pr-review": 2}
/// ```
///
/// Returns `(rule_file, suggestion)` if parsing succeeds.
pub fn parse_convention_context(context: &str) -> Option<(String, String)> {
    let mut rule_file = None;
    let mut suggestion = None;

    for line in context.lines() {
        if let Some(rest) = line.strip_prefix("Rule file: ") {
            rule_file = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Suggestion: ") {
            suggestion = Some(rest.trim().to_string());
        }
    }

    match (rule_file, suggestion) {
        (Some(rf), Some(sg)) => Some((rf, sg)),
        _ => None,
    }
}

/// Apply approved convention updates from HITL responses.
///
/// Scans all responded HITL events whose situation starts with "Convention update suggested:",
/// checks the response choice, and writes or skips rule files accordingly.
///
/// - choice=1 ("Apply"): write the suggestion to the rule file
/// - choice=2 ("Edit and apply"): use the response message as content
/// - choice=3 ("Reject"): skip, mark pattern as Rejected
///
/// After processing each event, its status is set to `Applied` so it is not
/// re-processed on subsequent invocations.
///
/// Returns a summary string.
pub fn apply_approved(
    db: &Database,
    repo_name: &str,
    repo_id: &str,
    repo_path: &Path,
) -> Result<String> {
    use crate::core::models::HitlStatus;
    use crate::core::repository::HitlRepository;

    let events = db.hitl_list(Some(repo_name))?;

    let mut applied = 0u32;
    let mut rejected = 0u32;
    let mut skipped = 0u32;

    for event in &events {
        if !matches!(event.status, HitlStatus::Responded) {
            continue;
        }

        if !event.situation.starts_with("Convention update suggested:") {
            continue;
        }

        let responses = db.hitl_responses(&event.id)?;
        let response = match responses.last() {
            Some(r) => r,
            None => continue,
        };

        let choice = match response.choice {
            Some(c) => c,
            None => continue,
        };

        let parsed = parse_convention_context(&event.context);

        match choice {
            1 => {
                // Apply: use the original suggestion
                if let Some((rule_file, suggestion)) = parsed {
                    write_rule_file(repo_path, &rule_file, &suggestion)?;
                    update_linked_pattern_status(
                        db,
                        repo_id,
                        &event.situation,
                        FeedbackPatternStatus::Applied,
                    )?;
                    db.hitl_set_status(&event.id, HitlStatus::Applied)?;
                    applied += 1;
                } else {
                    skipped += 1;
                }
            }
            2 => {
                // Edit and apply: use the response message as content
                if let Some((rule_file, _)) = parsed {
                    let content = response.message.as_deref().unwrap_or("").trim();
                    if content.is_empty() {
                        skipped += 1;
                    } else {
                        write_rule_file(repo_path, &rule_file, content)?;
                        update_linked_pattern_status(
                            db,
                            repo_id,
                            &event.situation,
                            FeedbackPatternStatus::Applied,
                        )?;
                        db.hitl_set_status(&event.id, HitlStatus::Applied)?;
                        applied += 1;
                    }
                } else {
                    skipped += 1;
                }
            }
            3 => {
                // Reject
                update_linked_pattern_status(
                    db,
                    repo_id,
                    &event.situation,
                    FeedbackPatternStatus::Rejected,
                )?;
                db.hitl_set_status(&event.id, HitlStatus::Applied)?;
                rejected += 1;
            }
            _ => {
                skipped += 1;
            }
        }
    }

    Ok(format!(
        "Convention apply complete: {applied} applied, {rejected} rejected, {skipped} skipped\n"
    ))
}

/// Write (or append) a suggestion to a convention rule file.
fn write_rule_file(repo_path: &Path, rule_file: &str, suggestion: &str) -> Result<()> {
    let target = repo_path.join(rule_file);

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if target.exists() {
        // Append as a new section
        let existing = std::fs::read_to_string(&target)?;
        let mut content = existing;
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(suggestion);
        content.push('\n');
        std::fs::write(&target, content)?;
    } else {
        // Create with a heading derived from the filename
        let stem = std::path::Path::new(rule_file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Convention");
        let heading = stem
            .split('-')
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let content = format!("# {heading}\n\n{suggestion}\n");
        std::fs::write(&target, content)?;
    }

    Ok(())
}

/// Update the linked feedback pattern status based on the HITL event situation.
///
/// The situation has format "Convention update suggested: <pattern_type>".
/// We find the pattern with that type and status=Proposed, then update it.
fn update_linked_pattern_status(
    db: &Database,
    repo_id: &str,
    situation: &str,
    status: FeedbackPatternStatus,
) -> Result<()> {
    let pattern_type = situation
        .strip_prefix("Convention update suggested: ")
        .unwrap_or("");

    if pattern_type.is_empty() {
        return Ok(());
    }

    // Find the proposed pattern matching this type
    let patterns = db.feedback_list(repo_id)?;
    for p in &patterns {
        if p.pattern_type == pattern_type && p.status == FeedbackPatternStatus::Proposed {
            db.feedback_set_status(&p.id, status.clone())?;
            break;
        }
    }

    Ok(())
}

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
