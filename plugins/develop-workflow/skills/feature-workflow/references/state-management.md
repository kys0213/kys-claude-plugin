# 상태 관리 가이드

`.develop-workflow/state.yaml` 1개 파일로 워크플로우 상태를 추적합니다.
compaction이나 세션 재개 시 "어디까지 했는지"를 복원하기 위한 최소 정보만 기록합니다.

## 설계 원칙

1. **파일 1개만**: `.develop-workflow/state.yaml`이 유일한 상태 파일
2. **최소 기록**: "어디까지 했는지"만 기록. 피드백 상세, 설계 내용은 기록하지 않음
3. **재분석 가능**: 실패 원인 등은 다시 분석하면 되므로 결과만 기록
4. **Phase 전환 + RALPH iteration 경계에서만 갱신**: 불필요한 I/O 최소화

## state.yaml 스키마

```yaml
# .develop-workflow/state.yaml

phase: IMPLEMENT              # DESIGN | REVIEW | IMPLEMENT | MERGE | DONE
strategy: subagent            # direct | subagent | agent-teams (Phase 3 전용)
feature: "실시간 채팅 기능"     # 사용자 요청 요약 (세션 재개 시 컨텍스트)
started_at: "2026-02-16T10:00:00"
updated_at: "2026-02-16T11:30:00"

checkpoints:                  # Phase 2에서 Checkpoint 정의 후 초기화
  cp-1: { status: passed, iteration: 2 }
  cp-2: { status: in_progress, iteration: 1 }
  cp-3: { status: pending, iteration: 0 }
```

### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `phase` | enum | 현재 워크플로우 Phase |
| `strategy` | enum | 구현 전략 (Phase 3 진입 시 결정) |
| `feature` | string | 사용자 요청 요약 (재개 시 컨텍스트) |
| `started_at` | ISO 8601 | 워크플로우 시작 시각 |
| `updated_at` | ISO 8601 | 마지막 갱신 시각 |
| `checkpoints` | map | Checkpoint별 상태 |

### Checkpoint 상태

| status | 의미 |
|--------|------|
| `pending` | 아직 시작하지 않음 |
| `in_progress` | RALPH 루프 실행 중 |
| `passed` | 검증 통과 완료 |
| `escalated` | 재시도 초과, 사용자 개입 필요 |

### iteration 필드

- `0`: 아직 시작하지 않음
- `1~3`: RALPH 몇 번째 시도인지

## 상태 기록 시점

### Phase 전환 시

```
Phase 1 시작 → Write state.yaml (phase: DESIGN, feature 기록)
Phase 2 진입 → Edit state.yaml (phase: REVIEW, checkpoints 초기화)
Phase 3 진입 → Edit state.yaml (phase: IMPLEMENT, strategy 기록)
Phase 4 진입 → Edit state.yaml (phase: MERGE)
완료          → Edit state.yaml (phase: DONE)
```

### RALPH iteration 경계에서

```
CP 시작     → Edit state.yaml (cp-N: in_progress, iteration: 1)
검증 통과   → Edit state.yaml (cp-N: passed)
검증 실패   → Edit state.yaml (cp-N: iteration 증가)
재시도 초과 → Edit state.yaml (cp-N: escalated)
```

## 세션 재개 흐름

```
/develop 실행
    │
    ├── state.yaml 없음 → 새 워크플로우
    │
    ├── phase: DONE → "이전 완료. 새로 시작합니다." → state.yaml 삭제 → 새 워크플로우
    │
    └── phase: DESIGN | REVIEW | IMPLEMENT | MERGE
        │
        ▼
        사용자에게 보고:
        "이전 세션: '{feature}', Phase {phase}"
        "Checkpoints: cp-1 passed, cp-2 in_progress (iter 1/3), cp-3 pending"
        │
        ├── AskUserQuestion: "이어서 진행" → 해당 지점부터 재개
        └── AskUserQuestion: "처음부터"   → state.yaml 삭제 → 새 워크플로우
```

### 재개 시 Phase별 동작

| Phase | 재개 동작 |
|-------|----------|
| DESIGN | Phase 1부터 재시작 (설계는 컨텍스트 필요하므로) |
| REVIEW | Phase 2부터 재시작 (리뷰 재실행) |
| IMPLEMENT | passed 건너뜀, in_progress CP의 현재 iteration부터 |
| MERGE | Phase 4부터 재시작 |

> **IMPLEMENT가 핵심**: DESIGN/REVIEW/MERGE는 상대적으로 짧으므로 재시작해도 됨.
> IMPLEMENT는 오래 걸리므로 Checkpoint 단위 재개가 중요함.

## Compaction 대응

Claude Code가 컨텍스트를 압축할 때:
1. 대화 히스토리의 상세 내용이 요약됨
2. **state.yaml은 파일이므로 영향 없음**
3. 압축 후 state.yaml을 읽으면 "어디까지 했는지" 복원 가능
4. 피드백 상세는 유실되지만, 다시 분석하면 됨

```
Compaction 발생
    │
    ▼
컨텍스트 요약됨 (피드백 상세 유실)
    │
    ▼
state.yaml 읽기 → "cp-2가 iteration 1에서 in_progress"
    │
    ▼
cp-2의 현재 코드와 테스트를 다시 읽음
    │
    ▼
RALPH 계속 (피드백은 재분석)
```

## 파일 위치

```
프로젝트 루트/
└── .develop-workflow/
    └── state.yaml        ← 이것만
```

> `.gitignore`에 `.develop-workflow/` 추가를 권장합니다.
> 이 상태 파일은 로컬 개발 세션 전용이며 커밋 대상이 아닙니다.
