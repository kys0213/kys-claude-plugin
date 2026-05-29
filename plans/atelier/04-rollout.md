# Atelier — 롤아웃 & 검증

> **상태**: 설계 단계 (00~03 의 후속, 최종 doc)
> **입력**: 02(아키텍처), 03(마이그레이션)
> **출력**: 실제 구현 PR 시퀀스

이 문서는 atelier 통합을 어떤 순서의 PR 로 쪼개고, CI 게이트를 어떻게 구현하며,
각 단계에서 무엇을 검증하는지 정의한다. CLAUDE.md 의 \"Design First\" 원칙에 따라
이 doc 승인 후에야 구현 PR 을 시작한다.

---

## 1. PR 분할 원칙

- **각 PR 은 독립적으로 머지 가능하고 CI 가 green** 이어야 한다.
- atelier 가 Phase 4 까지 동작 확인되기 전까지는 **frozen 적용을 하지 않는다** (롤백 가능성 확보). marketplace 에 atelier 자체는 Phase 1 부터 \"WIP\" 라벨로 등록된다 — validate 도구의 marketplace-sync 강제 때문 (Phase 1 결정 참고).
- CLI 포팅(Phase 2)은 가장 큰 작업이므로 단독 PR. TDD 로 진행 (02 §4.3 의 테스트 이전 포함).
- 각 PR title 은 git-workflow.md 형식 + 버전 범프 규칙 준수.

---

## 2. Phase 시퀀스

### Phase 0 — 사전 검증 (코드 변경 없음)

| 항목 | 방법 | 산출 |
|---|---|---|
| marketplace schema `deprecated`/`replacedBy` 지원 | `make validate` + 로컬 schema 확보 또는 strict 검증 | 02 §7.3 폴백 적용 여부 확정 |
| `[package.metadata.ci]` 빌드 매트릭스 형식 | `suggest-workflow`, `autodev` Cargo.toml 참조 | atelier Cargo.toml ci 메타 작성 기준 |
| autopilot CLI 외부 호출 grep | `rg '\bautopilot\b' --type-not md` | alias/바이너리 영향 범위 |

> PR 아님. 이슈 코멘트로 결과 기록. 폴백이 필요하면 02 §7.3 자동 적용.

### Phase 1 — atelier 골격 (`feat`)

`feat(atelier): scaffold plugin skeleton`

- `plugins/atelier/.claude-plugin/plugin.json` (`0.1.0`)
- `plugins/atelier/README.md` 초안 (6→1 매핑표 + 슬래시 표면 placeholder)
- **marketplace.json 에 atelier 0.1.0 entry 등록** (description 에 \"WIP / Epic 1 진행 중\" 명시)
- 검증: `make validate` 통과 (marketplace-sync 포함)

> **결정 근거**: `tools/validate` 의 marketplace-sync 체크는 `plugin.json` 존재 시 marketplace 등록을
> **invariant 로 강제**한다. 04 초안의 \"Phase 1 = marketplace 노출 안 함\" 분리는 도구가 허용하지
> 않아 폐기. 빈 plugin 이 노출돼도 실해는 없으며(install 시 0개 capability), description 으로
> 의도가 명시된다. Phase 5a 의 marketplace 등록은 본 단계로 흡수되고, Phase 5 는 freeze 작업만 남는다.

### Phase 2 — CLI 통합 (`feat`, 최대 작업)

`feat(atelier): unify CLI into single rust binary`

순서 (02 §4.3, TDD):

```
2a. autopilot Rust crate 를 plugins/atelier/cli/ 로 이동
    - Cargo.toml: name="atelier", bin="atelier"
    - src/autopilot/ 하위로 기존 모듈 이동, lib.rs/main.rs 재구성
    - [package.metadata.ci].targets 추가 (rust-binary.yml 빌드용)
2b. clap 최상위 라우터(cli.rs): atelier autopilot <...> 동작 복원
    - 기존 autopilot 테스트 전부 green 유지
2c. git-utils 포팅 테스트 먼저 작성 (bun test → #[test]/assert_cmd)
    - 11개 테스트를 Rust 로 이전, 처음엔 fail
2d. git-utils core/commands Rust 구현 → 테스트 green
2e. atelier git <...> 라우팅 연결
2f. plugins/git-utils/src, package.json, tsconfig 의존 정리
    - 루트 tsc/eslint 대상에서 제외 (02 §4.5)
```

검증:
- `cargo test` (atelier crate) green — autopilot + git 양쪽
- `cargo fmt --check`, `cargo clippy -- -D warnings` (CLAUDE.md 품질 게이트)
- `atelier autopilot --help`, `atelier git --help` 동작

### Phase 3 — commands/agents/skills/hooks 이동 (`feat`)

`feat(atelier): migrate commands, agents, skills, hooks`

- 02 §1 구조대로 4개 그룹 폴더로 이동
- rename 적용 (02 §1.2)
- setup 통합: 단일 `commands/setup.md` + 모듈 선택 (02 §3)
- 중복 제거: `spec-validator` 제거, `gap-auditor` 로 책임 통합 (02 §5.4)
- hook 4종을 `atelier/hooks/` 로 + setup 의 hook 재작성 로직 구현 (03 §A.3)
- **namespace 치환**: 13건 cross-plugin 참조 접두사 제거 (02 §2.2)

검증 (§5 체크리스트 핵심):
- cross-plugin 접두사 grep 0건
- 슬래시 충돌 0건 (setup 단일화 확인)
- skill 이름 10개 유지 확인

### Phase 4 — CI 인프라 (`ci`)

`ci(atelier): add validation and binary build for atelier`

- `validate.yml` 에 atelier Rust 체크 블록 추가 (fmt/clippy/check/test) — 기존 suggest-workflow/autodev 패턴 복제 (§3.1)
- `rust-binary.yml` 은 태그 기반 자동 감지라 Cargo.toml 의 `[package.metadata.ci]` 만 맞으면 동작 — 추가 변경 최소
- frozen 게이트 추가 (§3.2)
- bumpversion freeze 제외 목록 (§4)

### Phase 5 — freeze (`docs`)

`docs(atelier): freeze absorbed plugins`

- 6개 README 에 ❄️ 배지 (03 §B.2)
- marketplace 6개 entry 에 `deprecated`/`replacedBy` (또는 폴백)
- atelier marketplace entry 의 description 갱신 (\"WIP\" → 정식 안내, Phase 1 의 placeholder 제거)
- `docs` type 이라 frozen plugin 버전 범프 안 됨 (03 §B.3)

> Phase 1 흡수로 \"5a publish\" 단계가 사라졌다. 5 는 freeze 단독.
> atelier 가 Phase 4 까지 동작 검증된 뒤에만 진행.

---

## 3. CI 게이트 구현

### 3.1 atelier Rust 검증 (validate.yml)

기존 `check_rust` step 에 atelier 감지 추가:

```yaml
if echo "$CHANGED" | grep -q '^plugins/atelier/'; then
  echo "atelier_changed=true" >> $GITHUB_OUTPUT
fi
```

이후 fmt/clippy/check/test step 을 `plugins/atelier/cli` working-directory 로 복제 (suggest-workflow 블록과 동형).

### 3.2 frozen 경로 변경 차단

신규 step (validate.yml):

```yaml
- name: Block changes to frozen plugins
  run: |
    FROZEN="git-utils github-autopilot spec-kit workflow-guide coding-style orchestrator"
    CHANGED=$(git diff --name-only origin/main...HEAD)
    LABELS='${{ toJson(github.event.pull_request.labels.*.name) }}'
    # freeze-exception 라벨이 있으면 우회 (03 §B.4)
    if echo "$LABELS" | grep -q 'freeze-exception'; then
      echo "freeze-exception label present — skipping gate"; exit 0
    fi
    for p in $FROZEN; do
      # README.md 변경만은 허용 (배지 추가)
      VIOLATION=$(echo "$CHANGED" | grep "^plugins/$p/" | grep -v "^plugins/$p/README.md" || true)
      if [ -n "$VIOLATION" ]; then
        echo "::error::frozen plugin '$p' modified: $VIOLATION"
        echo "후속 개발은 plugins/atelier/ 에서 하세요. 긴급 패치는 freeze-exception 라벨."
        exit 1
      fi
    done
```

> 이 step 은 5b(freeze 적용) 머지 이후부터 의미. Phase 4 에서 추가하되, 6개가 frozen 으로 마킹되기 전(5b 전)에는 README 외 변경이 자연히 없으므로 무해.

### 3.3 PR title 게이트는 기존 유지

validate.yml 의 \"Check PR title format\" / \"version bump prefix\" step 은 atelier 에도 그대로 적용 (atelier 는 `plugins/` 하위라 코드 변경 시 feat/fix/refactor 강제).

---

## 4. bumpversion freeze 제외

`tools/bumpversion` 의 변경 감지(`changes.GetPluginsOnly`)에서 frozen 6개를 영구 제외.

구현 방향 (Go):
- freeze 목록을 상수 또는 `.freeze` 마커 파일로 정의
- `GetPluginsOnly` 결과에서 freeze 목록 필터링
- 단위 테스트: frozen plugin 변경이 감지 결과에서 빠지는지 (`tools/bumpversion` 의 기존 테스트 패턴)

> 이렇게 하면 03 §B.3 의 \"실수로 frozen 경로 변경 시 범프 방지\" 안전망이 코드로 보장된다.

---

## 5. 최종 검증 체크리스트

릴리즈(5b 머지) 전 전수 확인.

### 5.1 기능 동작

```
□ atelier 단독 설치 후 /atelier:setup 동작 (git/autopilot/style/all)
□ /atelier:git/* , /atelier:autopilot/* , /atelier:spec/* , /atelier:workflow/* 슬래시 노출
□ atelier autopilot <기존 명령> = 기존 autopilot 동작 동일 (회귀 0)
□ atelier git <기존 명령> = 기존 git-utils 동작 동일 (회귀 0)
□ hook 4종 트리거 정상 (SessionStart, Stop 등)
□ 기존 github-autopilot.local.md / ~/.git-workflow-env / CLAUDE.md 호환
□ autopilot SQLite store 기존 DB 호환 (03 §A.6)
```

### 5.2 namespace / 참조 정합

```
□ rg 'git-utils:|spec-kit:|github-autopilot:' plugins/atelier  → 0건
□ rg 'plugins/(git-utils|github-autopilot|spec-kit|workflow-guide|coding-style|orchestrator)/' plugins/atelier  → 0건 (문서 예시 경로까지 치환)
□ 슬래시 커맨드 이름 충돌 0건 (setup 단일)
□ skill 이름 10개 모두 보존 (git, orchestrator, convention-architect, ...)
□ agent 19개 (spec-validator 제거 확인)
```

### 5.3 마이그레이션

```
□ /atelier:setup 의 hook 재작성: frozen 경로 → atelier 경로 (03 §A.3)
□ 재작성 멱등성: 두 번 실행 시 변경 0건
□ settings.json 백업 생성 확인
□ alias 생성 동의 흐름 동작
□ 롤백 절차(03 §C) atelier README 에 문서화
```

### 5.4 freeze / CI

```
□ 6개 README ❄️ 배지
□ marketplace 6개 deprecated/replacedBy (또는 폴백)
□ frozen 경로 변경 PR 차단 (README 제외, freeze-exception 우회)
□ bumpversion 이 frozen 6개를 범프 대상에서 제외
□ docs type freeze PR 이 버전 범프 트리거 안 함
□ frozen 디렉토리 삭제되지 않음 (롤백 가능 보장)
```

### 5.5 품질 게이트 (CLAUDE.md)

```
□ cargo fmt --check (atelier)
□ cargo clippy -- -D warnings (atelier)
□ cargo test (atelier — autopilot + git)
□ make validate / make validate-ci
□ npm run check (잔여 TS — git-utils 제외 후 깨지지 않음)
```

---

## 6. 작업 추적

#738 (marketplace governance epic) 아래 **두 개의 sub-epic** (05 §7.1 의 단계 분리):

```
#738 marketplace governance
├── Epic 1: atelier consolidation   ← 이 문서(04) Phase 1~5. 이동 전용.
│     └ Phase 1~5 를 각각 sub-issue / task 로 분해.
└── Epic 2: atelier skill extraction ← 05 §4~6. Epic 1 머지·안정화 후 개설.
      └ spec-workflow / autopilot-pipeline / git references 추출 (도메인별 PR).
```

- 본 문서(04)의 Phase 1~5 는 **Epic 1** 에만 해당. Fat Controller 는 이동 단계에서 그대로 옮긴다.
- 각 PR title scope 는 `atelier` (코드) 또는 해당 도메인.
- Epic 2 는 Epic 1 완료 후 별도 착수 — 05 가 그 설계서.

## 7. 리스크 요약

| 리스크 | 영향 | 완화 |
|---|---|---|
| git-utils TS→Rust 포팅 회귀 | git 워크플로우 깨짐 | TDD (테스트 선이전), Phase 2 단독 PR |
| hook 경로 재작성 오류 | 사용자 hook 미동작 | 멱등성 + 백업 + diff 확인 (03 §A.3) |
| schema deprecated 미지원 | marketplace 검증 실패 | Phase 0 선검증 + 폴백 (02 §7.3) |
| frozen 실수 변경 → 범프 | 잘못된 릴리즈 | CI 게이트 + bumpversion 제외 (§3.2, §4) |
| autopilot 바이너리 이름 변경 | 외부 스크립트 깨짐 | alias + Phase 0 grep |

---

## 8. 설계 doc 완료

00~04 작성 완료. **이 시점에서 사용자 승인 → 구현 Phase 1 시작.**
승인 전까지 `plugins/atelier/` 실제 코드는 생성하지 않는다 (Design First).
