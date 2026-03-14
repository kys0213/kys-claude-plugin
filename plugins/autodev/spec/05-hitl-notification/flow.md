# Flow 5: HITL 알림

### 시나리오

Claw가 사람의 판단이 필요한 상황을 감지한다.

### HITL이 필요한 상황

| 상황 | 심각도 | 예시 |
|------|--------|------|
| 스펙 검증 실패 | High | 필수 섹션 누락 |
| 리뷰 반복 실패 | High | review_iteration ≥ 3 |
| 구현 실패 | High | ImplementTask 실패 |
| 스펙 간 충돌 | Medium | 같은 파일 수정 |
| 낮은 confidence | Medium | Claw 판단 confidence < threshold |
| 스펙 완료 판정 | Low | 최종 확인 요청 |

### 알림 아키텍처 (OCP)

알림 채널은 **Notifier trait**으로 추상화하여, 새 채널 추가 시 기존 코드 수정 없이 구현체만 추가한다.

```
trait Notifier
├── GitHubCommentNotifier   (기본, 항상 활성)
├── WebhookNotifier         (Slack, Discord 등 범용)
├── SlackNotifier           (Slack 네이티브 API)
└── (향후 추가: Email, Teams, Telegram, ...)
```

```rust
/// 알림 채널 추상화. 새 채널 추가 = 구현체 추가, 기존 코드 무변경.
pub trait Notifier: Send + Sync {
    /// 이 채널의 이름 (로깅/설정 매칭용).
    fn channel_name(&self) -> &str;

    /// HITL 알림을 전송한다.
    async fn notify(&self, event: &HitlEvent) -> Result<()>;
}

pub struct HitlEvent {
    pub repo_name: String,
    pub severity: HitlSeverity,      // High, Medium, Low
    pub situation: String,            // 상황 설명
    pub context: String,             // 상세 분석
    pub options: Vec<String>,         // 사용자 선택지
    pub work_id: Option<String>,      // 관련 이슈/PR
    pub spec_id: Option<String>,      // 관련 스펙
}
```

### 알림 설정

```yaml
# .autodev.yaml
notifications:
  channels:
    - type: github-comment             # 기본, 항상 포함
      config:
        mention: "@kys0213"

    - type: webhook                    # 범용 webhook (Slack incoming webhook 등)
      config:
        url: "https://hooks.slack.com/services/..."
        # severity_filter: high        # (옵션) high만 전송

    # - type: slack                    # Slack 네이티브 API (향후)
    #   config:
    #     channel: "#autodev"
    #     token_env: "SLACK_BOT_TOKEN"
```

### 채널별 동작

| 채널 | 전송 대상 | 포맷 | 기본 활성 |
|------|----------|------|----------|
| `github-comment` | 해당 이슈/PR | 마크다운 (상황 + 선택지) | ✅ 항상 |
| `webhook` | 설정된 URL | JSON payload | 설정 시 |
| `slack` | 설정된 채널 | Slack Block Kit | 설정 시 |

### 알림 발송 흐름

```
Claw: HITL 판단
    │
    ▼
HitlEvent 생성
    │
    ▼
NotificationDispatcher
    │
    ├─→ GitHubCommentNotifier.notify()   → 이슈/PR에 코멘트
    ├─→ WebhookNotifier.notify()         → POST JSON to URL
    └─→ (설정된 다른 Notifier들...)
```

`NotificationDispatcher`는 설정된 모든 채널에 순차 전송하고,
개별 채널 실패 시 로그만 남기고 다른 채널은 계속 전송한다.

### 알림 예시 (GitHub 코멘트)

```markdown
## 🔔 autodev: 사람 확인 필요

**상황**: PR #42의 리뷰-수정 사이클이 3회 반복되었습니다.

**분석**:
동일한 피드백("에러 핸들링 누락")이 반복되고 있어
구조적 문제일 가능성이 있습니다.

**선택지**:
1. 직접 코드를 확인하고 수정 방향을 코멘트로 안내
2. `autodev:skip` 라벨을 추가하여 이 PR을 중단
3. 관련 스펙을 업데이트하여 요구사항을 명확히

@kys0213
```

### 알림 예시 (Webhook JSON)

```json
{
  "event": "hitl_required",
  "repo": "org/repo",
  "severity": "high",
  "situation": "PR #42의 리뷰-수정 사이클이 3회 반복",
  "work_id": "pr:org/repo:42",
  "spec_id": "uuid-xxx",
  "options": ["직접 리뷰", "skip", "스펙 수정"],
  "url": "https://github.com/org/repo/pull/42"
}
```

---

### HITL 피드백 응답

피드백 유형에 따라 3가지 입력 방식을 제공한다.

| 피드백 유형 | 입력 방식 | 적합한 상황 |
|------------|----------|-----------|
| **단순 선택** | TUI 숫자키 | 재시도, skip, approve 등 |
| **지시/방향** | CLI 명령 | "Redis 대신 Memcached로", 방향 전환 |
| **코드 리뷰** | GitHub PR 코멘트 | 코드 레벨 수정 요청 (기존 v3 flow) |

#### 1. 단순 선택 (TUI)

대시보드에서 HITL 항목 선택 후 숫자키로 즉시 응답.

```
🔔 [HIGH] PR #42 리뷰 3회 반복
   "동일한 피드백(에러 핸들링 누락)이 반복되고 있습니다."

   [1: 재시도] [2: skip] [3: 스펙 수정]
   > 사용자가 2를 누름 → autodev:skip 라벨 추가 → HITL 해소
```

#### 2. 지시/방향 (CLI)

상세한 피드백이 필요한 경우 CLI로 메시지를 전달.

```bash
# HITL 대기 목록 확인
autodev hitl list
# ID       REPO          SEVERITY  SITUATION
# hitl-01  org/repo-a    HIGH      PR #42 리뷰 3회 반복
# hitl-02  org/repo-b    MED       스펙 충돌 감지

# 선택지 번호로 응답
autodev hitl respond hitl-01 --choice 1

# 메시지와 함께 응답 (Claw에게 방향 지시)
autodev hitl respond hitl-01 --message "에러 핸들링은 Result<T, AppError> 패턴으로 통일해줘"

# 레포별 HITL만 확인
autodev hitl list --repo org/repo-a
```

Claw는 `--message`의 내용을 다음 판단 시 컨텍스트로 포함하여 작업 방향에 반영한다.

#### 3. 코드 리뷰 (GitHub)

코드 레벨 피드백은 기존 v3 flow 그대로 사용.

```
사용자: PR에 리뷰 코멘트 작성
  → autodev가 changes-requested 감지
  → ImproveTask 실행
  → HITL 자동 해소 (라벨 기반)
```

GitHub 코멘트로 HITL에 응답하면 별도 라벨 조작 없이도 Claw가 감지할 수 있도록,
HITL 코멘트에 **응답 마커**를 포함한다:

```markdown
## 🔔 autodev: 사람 확인 필요 <!-- autodev:hitl:hitl-01 -->

...

**응답 방법**:
- 이 코멘트에 답글을 달면 Claw가 다음 판단 시 반영합니다
- 또는: `autodev hitl respond hitl-01 --choice N`
```

Claw는 다음 틱에서 해당 마커의 답글을 스캔하여 응답으로 인식.

#### TUI에서 CLI 안내

TUI에서 상세 피드백이 필요한 HITL 항목을 선택하면 CLI 명령어를 안내:

```
🔔 [HIGH] PR #42 리뷰 3회 반복

   [1: 재시도] [2: skip] [3: 스펙 수정] [m: 메시지 입력]
   > 사용자가 m을 누름

   상세 피드백을 입력하려면:
   autodev hitl respond hitl-01 --message "..."
```

---

### HITL 응답 저장 + 데몬 동기화

HITL 응답은 DB에 저장되어 daemon이 다음 틱에서 참조한다.

```
사용자 응답 (TUI/CLI/GitHub)
    │
    ▼
hitl_responses 테이블에 저장
    │
    ▼
daemon 다음 틱:
  → Claw가 미해결 HITL 확인
  → 응답이 있으면 해당 작업 재개
  → 응답이 없으면 대기 유지
```

| 응답 경로 | 저장 방식 |
|----------|----------|
| TUI 숫자키 | autodev CLI가 직접 DB에 기록 |
| CLI `hitl respond` | autodev CLI가 직접 DB에 기록 |
| GitHub 답글 | daemon이 다음 scan에서 마커 답글 감지 → DB 기록 |

### HITL 타임아웃

설정 가능한 타임아웃으로 장기 미응답을 처리한다.

```yaml
# .autodev.yaml
hitl:
  timeout_hours: 24        # 기본: 24시간
  timeout_action: remind   # remind (재알림) | skip | pause_spec
```

| timeout_action | 동작 |
|---------------|------|
| `remind` | 동일 채널로 리마인드 알림 발송 (기본) |
| `skip` | 해당 이슈/PR을 autodev:skip 처리 |
| `pause_spec` | 관련 스펙을 Paused 상태로 전환 |
