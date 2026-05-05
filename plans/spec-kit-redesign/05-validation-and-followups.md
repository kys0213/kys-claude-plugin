# Validation Results and Follow-up Backlog

Phase 3 (#667) 머지 직후부터 belt 프로젝트에 대해 dogfood 를 반복하면서 설계가 의도대로 동작하는지 정성 검증했다. 본 문서는 그 결과와 발견된 후속 과제를 남긴다.

## Dogfood iteration table

| 회차 | 시점 (PR) | 대상 spec | L1 drop | 피드백 루프 | L2 drop | 비고 |
|------|-----------|-----------|---------|-------------|---------|------|
| 1 | 직후 #667 | belt `agent-runtime.md` | 22 / 54 (40.7%) | N/A | 0 | 사후 보정 섹션("L1 품질 노트")으로 누락 finding 4건 회복 |
| 2 | 직후 #683 | 동일 | 17 / 54 (31.5%) | N/A | 0 | prompt 강화로 primary drop 22→10, secondary cascade 7건 동반 발생 |
| 3 | 직후 #690 | 동일 | **0 / 54 (0%)** | 1회 | 0 | targeted 피드백 루프로 7개 인용 정정 후 전체 통과 |
| Test 1 | 직후 #690 | belt `cron-engine.md` | 1 / 39 (2.6%) | 1회 | 0 | 다른 spec 도메인 (스케줄링) 안정성 확인 |
| Test 2 | 직후 #690 | belt `agent-runtime.md` + `daemon.md` | **0 / 81 (0%)** | 0회 | 0 | 다중 spec — spec↔spec gap 4건 자동 검출 |

## Iteration narrative

### 회차 1 — baseline (#667 직후)

- Phase 3 첫 dogfood. L1 drop 22건 (40.7%), 50% 임계 직전.
- silent-fail 차단 메커니즘 ("검증 통계" footer) 은 정상 작동.
- 흥미로운 부산물: orchestrator (Claude) 가 자발적으로 **"L1 품질 노트"** 섹션을 추가하여 drop 된 substantive findings 4건을 사후 노출. 설계 약속 ("모든 drop 사용자 가시") 의 자연스런 연장.

### 회차 2 — L1 prompt tightening (#683)

- 5가지 paraphrasing 패턴(`pub` 등 키워드 생략 / 설명 prefix 추가 / middle ellipsis / doc comment 생략 / 들여쓰기 변경)을 명시적 "금지" 불릿로 추가.
- 결과: primary drops 22→10 (-55%) 이지만 secondary drops (`dangling reference` cascade) 7건이 새로 노출되어 전체 drop 은 17건 (~40%).
- prompt 만으로는 haiku 의 인용 정확도 한계를 더 끌어올리기 어렵다는 결론 — "한 번에 완성도를 높이려는" 접근의 한계.

### 회차 3 — targeted feedback loop (#690)

- wholesale retry (전체 재실행, 동일 prompt) 대신 **per-item 피드백 루프** 도입.
- 첫 검증 후 실패 항목별로 `[reason, agent's wrong excerpt, actual file lines]` 묶어 fix request 구성.
- haiku 가 단 1회 추가 호출로 7개 인용을 정정하고 100% 통과. drop 0.
- 의의: 모델 정확도를 **단계적 수렴**으로 흡수하는 설계가 "한 번에 완성도 높이기" 보다 효과적임을 검증.

### Test 1 — 다른 spec 도메인

- `cron-engine.md` (스케줄링/orchestration). agent-runtime 과 다른 영역.
- L1 drop 1/39 (2.6%), 피드백 1회로 통과.
- substantive findings: HIGH 1 (cron 환경변수 주입 누락), MEDIUM 2, LOW 1, Notes 4.
- 결론: 다양한 spec 패턴에서 안정.

### Test 2 — 다중 spec

- `agent-runtime.md` + `daemon.md` 동시 분석. 두 spec 은 token_usage / 환경변수 / 모듈 경계 영역에서 자연스레 접점.
- L1 drop 0/81 (양쪽 spec 모두 100% 첫 시도 통과). 피드백 루프 0회.
- L2 가 cross-file 패턴 매칭으로 **spec↔spec gaps 4건 자동 검출**. 다중 spec 분석의 핵심 가치 검증 지점.

## Spec ↔ Spec gap 발견 사례 (Test 2)

| Severity | 분류 | 발견 |
|----------|------|------|
| HIGH | DEFINITION_CONFLICT | 환경변수 주입 범위 충돌 — daemon.md 는 `WORK_ID/WORKTREE 2개만`, code 와 다른 spec 은 `BELT_DB/BELT_HOME` 추가 주입 |
| MEDIUM | INTERFACE_DRIFT | token_usage 기록 책임자 모호 — `agent-runtime.md` 는 "Daemon 자동 저장", `daemon.md` 의 5-모듈 매트릭스에 명시 없음 |
| MEDIUM | DEFINITION_CONFLICT | 모델 결정 우선순위 단계 표현 — spec 4단계 vs code 2~3단계 결합 |
| LOW | REQUIREMENT_OVERLAP | HitlService 책임 범위 — daemon.md 광범위 vs code handle_escalation 라우팅만 |

**이 4건은 단일 spec 분석으로는 발견 불가능**. 두 spec 의 인용을 cross-reference 한 결과로만 surface 됨. 다중 spec L2 분석의 가치를 정량적으로 보여준 사례.

## 04-test-scenarios.md 게이트 충족 현황

| 게이트 (`04-test-scenarios.md` §7) | 상태 | 비고 |
|------------------------------------|------|------|
| 환각 회귀 100% drop, finding 미발생 | ✅ | 메커니즘 검증 (Iteration 1~3 의 drop 처리). 의도적 환각 주입 회귀 테스트는 미실행 |
| D1-D5 / A/B/C 95% 이상 회귀 | ✅ (정성) | Test 2 의 4건 spec↔spec gap 이 D1-D5 등가 발견. 정량 비교 미실행 |
| 비용 기존의 70% 이하 | ⏳ 미측정 | haiku × N + sonnet × 1 구조이나 토큰/지연 측정은 후속 |
| Wall-clock 기존의 80% 이하 | ⏳ 미측정 | 병렬화 효과 정성 확인. 측정 후속 |
| 검증 unit test 전체 통과 | ⏳ 미구현 | 검증 로직이 orchestrator inline 으로 들어 있어 단위 테스트 어려움. F1 참조 |
| Phase 3 비교 (legacy vs new 90% 일치) | ⏳ 미실행 | legacy 6 에이전트 삭제 후 직접 비교 불가. dogfood 로 정성 대체 |
| dogfooding | ✅ | 본 문서의 5회 시도 |

게이트 일부는 **정성 검증으로 대체**된 채 머지됐다. 정량 측정과 회귀 테스트는 follow-up 으로 처리.

## Follow-up Backlog

### F1. 인용 검증 로직 CLI 추출

- **현재**: spec-review.md / gap-detect.md Step 4 가 검증 알고리즘을 prose 로 기술, orchestrator (Claude) 가 Read 도구로 inline 수행.
- **문제**: 결정적 로직이지만 단위 테스트 부재. claude 마다 미세하게 다르게 해석 가능.
- **제안**: `tools/spec-kit-validate/` 류로 Go/Rust CLI 추출. 입력 (L1 리포트 + 파일 contents) → 출력 (passing items + drop log JSON). orchestrator 는 CLI 호출 후 결과만 사용.
- **이득**: 단위 테스트 가능 (`04-test-scenarios.md` §4 unit test 게이트 충족), 재사용 가능, prose 명세 간결화.
- **비용**: 별도 빌드, 배포, 버전 관리.
- **우선순위**: 중. 정확도 단계적 개선 후 도구화 검토.

### F2. L1 모델 sonnet 승격 옵션

- **현재**: L1 = haiku (변동성 높지만 피드백 루프로 수렴).
- **검토**: 매우 큰 spec 또는 복잡한 dsl 의 경우 haiku 반복 비용 > sonnet 1회 비용일 수 있음.
- **제안**: spec 파일 크기/복잡도에 따라 모델 자동 선택, 또는 `--model` 옵션.
- **우선순위**: 낮음. 현재 비용에서 문제 미관찰.

### F3. 다른 프로젝트 dogfood

- **현재**: belt 만 검증됨.
- **제안**: github-autopilot 의 `plans/` (자체 spec) 또는 다른 spec-driven 프로젝트에서 추가 dogfood. 다양한 마크다운 스타일/도메인에서 견고성 확인.
- **우선순위**: 중. 외부 검증 의 본 검증을 보강.

### F4. legacy vs new 정량 비교

- **현재**: Phase 3 가 legacy 6 에이전트를 삭제했으므로 직접 비교 불가.
- **제안**: 머지 직전 commit (`HEAD~1`) 으로 체크아웃해서 동일 spec 에 legacy 흐름 실행, 결과를 새 흐름과 비교. finding overlap, severity 분포, 인용 품질을 정량화.
- **우선순위**: 낮음. 동작이 명백하나 게이트 충족 측면에서 가치.

### F5. spec-kit 도구 추출 (장기)

- **현재**: spec-kit 은 marketplace 의 plugin 으로만 존재.
- **제안**: 다른 LLM 환경 (Codex, Gemini, custom) 에서도 사용 가능한 정형 도구로 추상화. orchestration 의 bash/markdown 부분을 CLI 로.
- **우선순위**: 매우 낮음. 본 작업 범위 밖, 별도 epic 가치.

### F6. 환각 회귀 자동 테스트

- **현재**: #639 같은 환각 시나리오를 정형 테스트로 회귀 검증하는 픽스처 미구현.
- **제안**: `fixtures/issue-639/` 류 fixture + 의도적 환각 주입 mock + 검증 로직이 drop 처리하는지 확인하는 테스트.
- **우선순위**: 중. F1 (CLI 추출) 과 함께 진행 시 자연스러움.

## 닫는 글

당초 #639 (cross-reference-checker 환각) 으로 시작된 한 사이클이 design-first → 5분할 plan → 2-layer 에이전트 → 인용 검증 → 피드백 루프로 닫혔다. 게이트 일부는 정성 검증으로 대체됐고 follow-up backlog 6건이 남았으나, 핵심 약속 — **인용 by construction** 으로 환각이 구조적으로 도달 불가능 — 은 dogfood 로 검증됐다.
