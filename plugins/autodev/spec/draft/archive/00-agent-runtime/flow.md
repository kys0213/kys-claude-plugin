# Flow 0: AgentRuntime trait

### 시나리오

LLM 실행 시스템(Claude, Gemini, Codex, ...)을 추상화한다.
새 LLM 추가 = 새 AgentRuntime impl, 코어 변경 0 (OCP).

### 현재 구조의 문제

```
core/task.rs:
  AgentRequest { session_opts: SessionOptions }  ← Claude 전용

infra/claude/:
  trait Claude → RealClaude                      ← Claude 하드코딩

main.rs:
  autodev agent → claude --print -p "..."        ← Claude 하드코딩
```

core가 infra(Claude)에 의존하고 있다.

### trait 정의

autodev가 **실제로 필요한 기능**에서 도출한 인터페이스:

```rust
/// core/runtime.rs

#[async_trait]
pub trait AgentRuntime: Send + Sync {
    fn name(&self) -> &str;
    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse;
    fn capabilities(&self) -> RuntimeCapabilities;
}

pub struct RuntimeRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub structured_output: Option<StructuredOutput>,
    pub session_id: Option<String>,
}

pub struct StructuredOutput {
    pub schema: String,
}

pub struct RuntimeResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub token_usage: Option<TokenUsage>,
    pub session_id: Option<String>,
}

pub struct RuntimeCapabilities {
    pub can_edit_files: bool,
    pub supports_structured_output: bool,
    pub supports_system_prompt: bool,
    pub supports_session_resume: bool,
    pub max_context_tokens: usize,
}
```

### 의존성 방향

```
core/runtime.rs (trait + DTO)
     ↑ impl
infra/runtimes/
  ├── claude.rs
  ├── gemini.rs
  ├── codex.rs
  └── custom.rs

core → infra 방향 의존 없음.
```

### core 옵션 → CLI 매핑 (각 런타임의 책임)

| core 옵션 | Claude | Gemini | Codex |
|-----------|--------|--------|-------|
| `system_prompt` | `--append-system-prompt` | prompt prepend | prompt prepend |
| `structured_output` | `--output-format json` + `--json-schema` | `--output-format json` | `--output-schema <file>` + `--json` |
| `working_dir` | `current_dir()` | `current_dir()` | `--cd <dir>` |
| `session_id` | `--resume <uuid>` | `--resume <id>` | `codex exec resume <id>` |

기능 미지원 시 각 런타임이 폴백 처리 (예: system_prompt → prompt prepend).

### RuntimeRegistry

```rust
pub struct RuntimeRegistry {
    runtimes: HashMap<String, Arc<dyn AgentRuntime>>,
    default_name: String,
    overrides: HashMap<TaskKind, String>,
}

impl RuntimeRegistry {
    pub fn resolve(&self, task_kind: TaskKind) -> Arc<dyn AgentRuntime> {
        let name = self.overrides.get(&task_kind).unwrap_or(&self.default_name);
        self.runtimes[name].clone()
    }
}
```

### 설정

```yaml
# .autodev.yaml
runtime:
  default: claude
  claude:
    model: sonnet
  overrides:
    analyze: claude
    implement: claude
    review: gemini
    claw_evaluate: claude
```

### 멀티턴 세션

```
요청 시 session_id: None   → 새 세션 시작
응답에  session_id: Some()  → 다음 턴에서 재사용

활용: Claw가 큐 평가 → advance 판단 → 같은 세션에서 후속 조치
```

### 확장 시나리오

```yaml
# Multi-LLM 리뷰
runtime:
  overrides:
    review: multi
  multi:
    runtimes: [claude, gemini, codex]
    strategy: consensus

# 비용 최적화
runtime:
  overrides:
    analyze: haiku
    implement: sonnet
    claw_evaluate: opus

# 로컬 LLM
runtime:
  default: ollama
  custom:
    ollama:
      command: "ollama"
      args_template: "run codellama {prompt}"
```

---

### 관련 플로우

- [Flow 0: DataSource](../00-datasource/flow.md)
- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md)
