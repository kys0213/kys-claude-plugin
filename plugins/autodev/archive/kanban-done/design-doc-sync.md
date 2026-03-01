# DESIGN.md 설계-구현 정합성 갱신 (M-1, M-2, L-1~L-3)

> **Priority**: Medium — 설계 문서가 실제 구현과 불일치
> **분석 리포트**: design-implementation-analysis.md §2-4, §2-6, §2-1, §2-2, §2-5
> **난이도**: 낮음

## 배경

구현이 설계보다 발전한 부분들이 DESIGN.md에 반영되지 않아 문서 정합성이 깨짐.
설계 문서를 실제 구현 상태로 갱신하여 신규 기여자나 미래 참조 시 혼란 방지.

## 항목

### Medium

- [ ] **1. Pre-flight check 설명 갱신 (M-1)**
  - DESIGN.md §5: "pre-flight API 호출 불필요" → 방어적 pre-flight check가 구현된 사유 기록
  - `notifier.is_issue_open()`, `is_pr_reviewable()`, `is_pr_mergeable()` 존재 반영
  - scan과 consume 사이 시간 차로 인한 방어적 체크 사유 설명

- [ ] **2. Merge scan 소스 갱신 (M-2)**
  - DESIGN.md §6: "approved 상태 + 라벨 없는 PR" → "autodev:done 라벨이 붙은 open PR"로 수정
  - done → wip 라벨 전환 후 merge queue push 흐름 반영

### Low

- [ ] **3. suggest_workflow 인프라 추가 (L-1)**
  - DESIGN.md §3 디렉토리 구조에 `infrastructure/suggest_workflow/` 추가
  - `SuggestWorkflow` trait + mock/real 구현체 기재

- [ ] **4. daemon.status.json 메커니즘 추가 (L-2)**
  - DESIGN.md §9 또는 §10에 status file 메커니즘 문서화
  - `DaemonStatus` 구조체, atomic write, CLI 연동 설명

- [ ] **5. DailyReport cross_analysis 스키마 갱신 (L-3)**
  - DESIGN.md §14: `DailyReport` JSON Schema에 `cross_analysis: Option<CrossAnalysis>` 추가
  - `CrossAnalysis { tool_frequencies, anomalies, sessions }` 구조체 기재

## 관련 파일

| 파일 | 변경 내용 |
|------|----------|
| `DESIGN.md` | §3, §5, §6, §9/§10, §14 갱신 |

## 완료 조건

- [ ] DESIGN.md의 설명이 실제 구현 동작과 일치
- [ ] 신규 모듈/구조체가 설계 문서에 반영됨
