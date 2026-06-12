---
description: "autopilot 루프를 설정된 모드로 시작합니다 (event-driven hybrid 또는 cron 기반)"
argument-hint: ""
allowed-tools: ["Read", "Bash", "CronCreate", "Monitor", "TaskStop"]
---

# Autopilot

설정의 `event_mode`에 따라 autopilot 루프를 시작합니다.

- **hybrid** (기본): Monitor 기반 이벤트 드리븐 + CronCreate 혼합
- **cron**: 기존 CronCreate 기반 폴링

이 커맨드는 진입점만 담습니다. 시작 절차 전체(preflight → 품질 게이트 → 초기 스캔 → 루프 등록 → 스냅샷)는 `autopilot` skill 의 `references/startup.md` 가 단일 소유합니다.

## 사용법

```bash
/atelier:autopilot
```

## Context

- 설정 파일: !`cat github-autopilot.local.md 2>/dev/null | head -20 || echo "설정 파일 없음 - 기본값 사용"`

## 실행 흐름

### Step 1: 설정 로딩

`github-autopilot.local.md`에서 설정을 읽습니다. 파일이 없거나 항목이 비어 있으면 `autopilot` skill `references/startup.md` §"기본 설정값" 의 기본값을 사용합니다.

### Step 2: 시작 절차 수행

`autopilot` skill 의 `references/startup.md` 를 로드하여 절차를 순서대로 수행합니다:

| 단계 | 내용 |
|------|------|
| Preflight | `autopilot preflight` CLI 환경 검증 (FAIL 시 `/atelier:setup` 안내 후 중단) |
| Spec Quality Gate | `spec_paths` 스펙 품질 평가, threshold 미달 시 사용자 확인 |
| 초기 스캔 | 기존 갭 분석 + 미분석 이슈 처리 + 세션 통계 초기화 |
| 모드 분기 | hybrid → Monitor + CronCreate / cron → CronCreate 전용 |
| Ledger 스냅샷 | backlog epic 상태 1회 출력 (정보성, failure isolation) |
| 결과 출력 | 등록된 Monitor/루프 요약 표 |

## 에러 처리

- **preflight FAIL**: FAIL 항목을 보여주고 `/atelier:setup` 안내 후 중단합니다.
- **spec 품질 threshold 미달**: AskUserQuestion 으로 진행 여부를 확인하고, 거부 시 `/atelier:spec` 안내 후 종료합니다.
- **Ledger 스냅샷 실패**: 정보성 단계이므로 경고만 남기고 autopilot 시작을 중단하지 않습니다.

## Output Examples

**hybrid 모드 시작:**

```
## Autopilot 시작 (hybrid 모드)
Monitor 1개 + CronCreate 2개 등록되었습니다.
```

**cron 모드 시작:**

```
## Autopilot 시작 (cron 모드)
9개 루프가 등록되었습니다.
CronList로 확인 가능합니다.
```

전체 출력 형식(Monitor 디스패치 표, 루프 표, Ledger 스냅샷)은 `references/startup.md` §"결과 출력" 을 따릅니다.
