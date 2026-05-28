# Atelier — 통합 plugin 컨셉

> **상태**: 설계 단계 (구현 전 승인 필요)
> **연관 epic**: #738 (marketplace governance)
> **브랜치**: `claude/vigilant-wright-1NOIk`

## 1. 배경

현재 `plugins/` 하위에는 13개 plugin이 있다. 그중 상당수는 **암묵적으로 서로를 호출**하거나 (예: `:setup` 슬래시가 plugin별로 흩어져 있음), **책임이 겹치거나** (예: spec-kit ↔ develop-workflow ↔ workflow-guide), **단독으로는 가치가 약한** 작은 도구들이다.

issue #738 (marketplace governance epic) 의 항목 (2)(3)(4) — 묵시적 의존 명시화, 중복 책임 통합, 테마 묶기 — 가 이 문제의 직접 해결을 요구한다.

## 2. 목적

\"개발 워크플로우를 처음부터 끝까지 책임지는 단일 큐레이션된 plugin\"을 만든다. 이름은 **atelier** (공방, 작가의 작업실) — 도구·재료·습관·작업 절차가 한 공간에 정돈된 메타포.

atelier 하나만 설치해도:

- spec 설계 → 리뷰 → 구현 → PR 머지까지의 전체 흐름이 끊김 없이 동작한다
- `:setup` 같은 진입점이 namespace 충돌 없이 단일 위치에서 제공된다
- 코딩 컨벤션·orchestrator 패턴·자율개발 루프가 동일한 어휘와 출력 형식으로 협력한다

## 3. 범위 결정

### 3.1 atelier로 흡수 (6개)

| plugin | 현재 버전 | 흡수 후 위치 (atelier 안) |
|---|---|---|
| `git-utils` | 0.2.0 | `commands/git/*`, `skills/git-utils/` |
| `github-autopilot` | 0.30.1 | `commands/autopilot/*`, `agents/autopilot-*`, `skills/autopilot-*` |
| `spec-kit` | 0.7.1 | `commands/spec/*`, `agents/spec-*`, `skills/spec-*` |
| `workflow-guide` | 0.6.0 | `skills/convention-architect`, `docs/workflow-principles.md` |
| `coding-style` | (TBD) | `skills/coding-style/` |
| `orchestrator` | 0.2.0 | `skills/orchestrator/`, `docs/orchestrator-pattern.md` |

근거: 모두 \"개발 사이클의 한 단계\"를 담당하며 서로를 묵시적으로 호출하고 있다. 단일 plugin 안에서 명시적 의존 그래프로 정리할 수 있다.

### 3.2 분리 active 유지 (3개 — 성격 다름)

| plugin | 사유 |
|---|---|
| `external-llm` | 외부 LLM 게이트웨이 — atelier의 도메인 외 인프라 |
| `barrier-sync` | FIFO 동기화 — 인프라 primitive |
| `openclaw-docker` | OpenClaw Docker 환경 관리 — DevOps |

### 3.3 일단 active 유지, atelier 완성 후 제거 검토 (4개)

| plugin | 향후 검토 시점 |
|---|---|
| `suggest-workflow` | atelier 워크플로우 제안 기능이 자체 흡수하면 제거 |
| `autodev` | autopilot이 사실상 대체 — atelier autopilot 안정화 후 제거 |
| `develop-workflow` | spec-kit + atelier 파이프라인으로 흡수되면 제거 |
| `hud` | UI 전용 — atelier와 무관하므로 별도 판단 |

이 4개는 **이번 atelier 작업에 포함하지 않는다**. atelier가 안정화된 뒤 별도 issue로 평가한다.

## 4. snapshot freeze 정책

흡수 대상 6개에 적용한다.

### 4.1 정의

> **snapshot freeze = 현재 버전을 마지막 버전으로 동결, 디렉토리는 그대로 유지, 후속 PR/version bump 없음.**

삭제 아님. marketplace에서 계속 설치 가능하고, 기존 사용자의 설치본은 그대로 동작한다.

### 4.2 운영 규칙

1. **디렉토리 유지**: `plugins/<name>/` 그대로 남긴다. 콘텐츠를 atelier로 *복사* 하되 *이동* 하지 않는다.
2. **README 배지**: 각 plugin README 상단에 다음을 추가한다.
   ```
   > ❄️ **Snapshot freeze** — 이 plugin은 v<X.Y.Z>에서 동결되었습니다.
   > 후속 개발은 [atelier](../atelier/) 에서 진행됩니다.
   ```
3. **CI 게이트**: `plugins/<frozen-name>/` 경로 변경을 포함하는 PR은 차단한다 (예외: README 배지 추가, 보안 패치).
4. **marketplace.json**: 6개 entry에 `"deprecated": true` 또는 동등한 표식을 둔다 (스키마 확인 필요 — 01-inventory에서 조사).
5. **atelier README**: 6개 plugin → atelier 매핑 표를 포함한다. 마이그레이션 가이드는 `plans/atelier/03-migration.md` 에서 상세화.

### 4.3 무엇이 frozen에 포함되지 않는가

- 분리 active 유지하는 3개 (external-llm, barrier-sync, openclaw-docker)
- 추후 제거 검토 대상 4개 (suggest-workflow, autodev, develop-workflow, hud)

이들은 frozen이 아니므로 평소처럼 PR / version bump를 받는다.

## 5. 비범위 (Out of Scope)

이번 atelier 작업에서 **하지 않을** 것을 명시한다.

- 4개 \"추후 검토\" plugin의 제거 또는 흡수
- marketplace.json 스키마 자체 개편 (deprecated 필드 추가는 필요 시 별도 PR)
- 흡수 6개 plugin의 기능 *변경* (단순 복사 + 경로 재조정만, 동작은 동일하게 유지)
- 자동 마이그레이션 도구 (사용자가 직접 install / uninstall — 가이드 문서로 충분)

## 6. 성공 기준

다음을 모두 만족하면 atelier 작업이 완료된 것으로 본다.

1. `atelier` plugin 단독 설치만으로 흡수 6개 plugin의 모든 commands/skills/agents/hooks가 정상 동작한다.
2. 6개 frozen plugin이 marketplace에서 \"deprecated\" 로 표시되고 README 배지가 붙는다.
3. 슬래시 namespace 충돌이 0건 (`:setup` 등 통합 처리 완료).
4. 마이그레이션 가이드 문서가 있고, 실제로 따라 했을 때 기존 환경에서 atelier로 옮길 수 있다.
5. CI에서 frozen 6개 경로의 변경이 차단된다.

## 7. 후속 설계 doc

승인되면 다음 순서로 작성한다.

| 파일 | 내용 |
|---|---|
| `01-inventory.md` | 6개 plugin의 commands/skills/agents/hooks 인벤토리 + cross-plugin 참조 매트릭스 + 슬래시 충돌 매트릭스 |
| `02-architecture.md` | atelier 디렉토리 구조, namespace 정책, hook 경로 마이그레이션, marketplace.json 변경안 |
| `03-migration.md` | 기존 사용자 마이그레이션 가이드 + freeze 운영 절차 |
| `04-rollout.md` | 단계별 PR 분할, CI/CD scope 규칙 업데이트, 검증 체크리스트 |

## 8. 결정 대기 사항

이 doc 승인 시 함께 확인이 필요한 항목:

- [ ] 이름 `atelier` 확정 (대안: workshop, studio, forge, suite — 변경 시 후속 doc 명명도 조정)
- [ ] 첫 릴리즈 버전 정책: atelier `0.1.0` 신규 시작 vs 흡수 plugin 중 가장 높은 버전 계승
- [ ] frozen 6개에 대한 marketplace.json `deprecated` 필드 — 스키마 지원 여부 확인 결과에 따라 정책 변경 가능
