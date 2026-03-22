# AgentRuntime — LLM 실행 추상화

> LLM 실행 시스템(Claude, Gemini, Codex, ...)을 추상화한다.
> 새 LLM 추가 = 새 AgentRuntime impl, 코어 변경 0 (OCP).

---

## trait 정의

```rust
#[async_trait]
pub trait AgentRuntime: Send + Sync {
    fn name(&self) -> &str;
    async fn invoke(&self, request: RuntimeRequest) -> RuntimeResponse;
    fn capabilities(&self) -> RuntimeCapabilities;
}

pub struct RuntimeRequest {
    pub working_dir: PathBuf,
    pub prompt: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub structured_output: Option<StructuredOutput>,
    pub session_id: Option<String>,
}

pub struct RuntimeResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub token_usage: Option<TokenUsage>,
    pub session_id: Option<String>,
}
```

---

## 의존성 방향

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

---

## 모델 결정 우선순위

```
1. RuntimeRequest.model        ← 호출 시 명시 (최우선)
2. handler의 runtime/model     ← DataSource state config
3. .autodev.yaml의 runtime 기본값
4. 런타임 내장 기본 모델
```

---

## core 옵션 → CLI 매핑

| core 옵션 | Claude | Gemini | Codex |
|-----------|--------|--------|-------|
| `model` | `--model <model>` | `-m <model>` | `-m <model>` |
| `system_prompt` | `--append-system-prompt` | prompt prepend | prompt prepend |
| `structured_output` | `--output-format json` + `--json-schema` | `--output-format json` | `--output-schema <file>` + `--json` |
| `working_dir` | `current_dir()` | `current_dir()` | `--cd <dir>` |
| `session_id` | `--resume <uuid>` | `--resume <id>` | `codex exec resume <id>` |

---

## RuntimeRegistry

```rust
pub struct RuntimeRegistry {
    runtimes: HashMap<String, Arc<dyn AgentRuntime>>,
    default_name: String,
}

impl RuntimeRegistry {
    pub fn resolve(&self, name: &str) -> Arc<dyn AgentRuntime> {
        self.runtimes.get(name)
            .unwrap_or(&self.runtimes[&self.default_name])
            .clone()
    }
}
```

어떤 런타임을 사용할지는 DataSource state config의 handler에서 지정한다.

---

## 설정

```yaml
runtime:
  default: claude
  claude:
    model: sonnet
  gemini:
    model: pro
```

---

## Handler에서의 사용

DataSource state의 handler가 prompt 타입이면 AgentRuntime.invoke()를 경유:

```yaml
states:
  analyze:
    handlers:
      - prompt: "이슈를 분석해줘"
        runtime: claude          # 이 handler는 Claude 사용
        model: haiku             # haiku 모델로
      - prompt: "PR을 리뷰해줘"
        runtime: gemini          # 이 handler는 Gemini 사용
```
