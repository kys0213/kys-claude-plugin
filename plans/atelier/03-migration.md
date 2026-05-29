# Atelier — 마이그레이션 & freeze 운영

> **상태**: 설계 단계 (02-architecture.md 의 후속)
> **대상 독자**: 기존 6개 plugin 사용자 + 메인테이너
> **선행 조건**: 02 §3(setup), §4(CLI), §7(marketplace) 확정

이 문서는 (A) 기존 사용자가 frozen 6개에서 atelier 로 옮겨가는 절차와,
(B) 메인테이너가 frozen 상태를 운영하는 절차를 정의한다.

---

## A. 사용자 마이그레이션

### A.1 마이그레이션이 필요한 자산

설치되어 있던 6개 plugin 사용자는 다음 3종류의 상태를 atelier 로 옮겨야 한다.

| 상태 | 어디에 박혀 있나 | 마이그레이션 방식 |
|---|---|---|
| 설치된 hook | `~/.claude/settings.json` (frozen plugin 경로) | `/atelier:setup` 이 자동 재작성 (A.3) |
| CLI 바이너리 | `autopilot`, `git-utils` (dist) | atelier 단일 바이너리 + alias (A.4) |
| 프로젝트 설정 파일 | `github-autopilot.local.md`, `~/.git-workflow-env`, `~/.claude/CLAUDE.md` | 그대로 호환 — 경로/포맷 변경 없음 (A.5) |

### A.2 전체 흐름

```
1. atelier 설치          claude plugin install atelier
2. /atelier:setup 실행    → hook 재작성 + CLI 빌드/설치 + alias 생성
3. 동작 확인             → 새 슬래시(/atelier:...) + atelier CLI 검증
4. (선택) 기존 6개 제거   claude plugin uninstall github-autopilot ...
```

> frozen 디렉토리는 marketplace 에 남아 있으므로 4번을 하지 않아도 충돌 없이 공존한다. 단, 슬래시가 중복 표시되므로 제거를 권장한다.

### A.3 hook 재작성 (핵심)

01 §5.1 의 리스크 대응. 기존 settings.json 에는 frozen 경로가 박혀 있다:

```jsonc
// 마이그레이션 전 (frozen 경로)
{
  "hooks": {
    "SessionStart": [
      { "command": ".../plugins/github-autopilot/hooks/check-cli-version.sh" }
    ],
    "Stop": [
      { "command": ".../plugins/coding-style/hooks/suggest-simplify.sh" }
    ]
  }
}
```

`/atelier:setup` 의 hook 재작성 알고리즘:

```
1. ~/.claude/settings.json 읽기 (없으면 skip)
2. hooks 의 모든 command 문자열을 순회
3. 다음 정규식에 매칭되는 entry 를 수집:
     .*/plugins/(github-autopilot|coding-style)/hooks/(<file>)\.sh
4. 매칭된 각 entry 를 atelier 경로로 치환:
     → ${CLAUDE_PLUGIN_ROOT}/hooks/<file>.sh
   (CLAUDE_PLUGIN_ROOT 는 atelier setup 실행 컨텍스트에서 atelier 로 해석됨)
5. 중복 제거: 같은 hook 이 frozen + atelier 양쪽에 있으면 atelier 만 남김
6. 변경 전 settings.json 을 settings.json.bak-<timestamp> 로 백업
7. 사용자에게 diff 를 보여주고 AskUserQuestion 으로 확인 후 기록
```

> **멱등성**: 이미 atelier 경로로 재작성된 settings.json 에 재실행하면 변경 0건. CLI 가 아니라 setup 커맨드(지능 계층)가 수행하므로 diff 확인 단계를 포함한다 (CLAUDE.md 책임 경계: 판단은 Skill/커맨드, 결정적 치환은 CLI 호출에 위임 가능).

치환 대상 hook 4종:

| frozen 경로 | atelier 경로 |
|---|---|
| `github-autopilot/hooks/check-cli-version.sh` | `atelier/hooks/check-cli-version.sh` |
| `github-autopilot/hooks/protect-stagnation.sh` | `atelier/hooks/protect-stagnation.sh` |
| `github-autopilot/hooks/guard-pr-base.sh` | `atelier/hooks/guard-pr-base.sh` |
| `coding-style/hooks/suggest-simplify.sh` | `atelier/hooks/suggest-simplify.sh` |

> git-utils 의 Default Branch Guard hook 도 동일 규칙. git-utils 는 hook 디렉토리가 없고 `scripts/` 의 `default-branch-guard-*.sh` 를 setup 이 등록했으므로, 정규식에 `git-utils/scripts/default-branch-guard-.*\.sh` 패턴을 추가한다.

### A.4 CLI 바이너리 전환

02 §4.4 확정. atelier 는 단일 `atelier` 바이너리.

```bash
# /atelier:setup 의 git / autopilot 모듈이 수행
cargo build --release --manifest-path "${CLAUDE_PLUGIN_ROOT}/cli/Cargo.toml"
# 바이너리를 PATH 에 링크 (예: ~/.local/bin/atelier)
```

기존 호출 호환을 위한 alias (setup 이 셸 rc 에 추가 제안):

```bash
alias autopilot='atelier autopilot'
alias git-utils='atelier git'
```

> alias 자동 추가는 AskUserQuestion 으로 동의받은 경우에만. 거부 시 안내 문구만 출력.

기존 `autopilot` 바이너리는 사용자가 직접 제거 (setup 이 제거하지 않음 — 외부 도구를 함부로 지우지 않는다).

### A.5 그대로 호환되는 설정

다음은 경로/포맷 변경이 없어 **마이그레이션 불필요**:

- `github-autopilot.local.md` — atelier autopilot 모듈이 동일 경로/스키마로 읽음
- `~/.git-workflow-env` — atelier git 모듈이 동일하게 읽음
- `~/.claude/CLAUDE.md` — coding-style 의 워터마크 기반 중복 확인 로직 그대로

### A.6 SQLite store 데이터

autopilot CLI 는 `rusqlite` 기반 store 를 사용 (01 §2.2, Cargo.toml). atelier 바이너리는 동일 DB 스키마/경로를 계승하므로 기존 ledger/task DB 를 그대로 읽는다. **DB 마이그레이션 불필요** (스키마 변경 시 04 의 검증 항목으로 별도 확인).

---

## B. freeze 운영 (메인테이너)

### B.1 freeze 적용 체크리스트 (6개 각각)

```
□ README 상단에 ❄️ Snapshot 배지 추가
□ marketplace.json entry 에 deprecated: true + replacedBy: "atelier"
   (schema 미지원 시 04 Phase 0 폴백 — description ❄️ 배지만)
□ CI 게이트: 해당 경로 변경 차단 규칙 등록 (04 §3)
□ bumpversion 자동 감지 제외 목록에 추가 (B.3)
```

### B.2 README 배지 표준 문구

각 frozen plugin README 최상단:

```markdown
> ❄️ **Snapshot freeze** — 이 플러그인은 v<X.Y.Z> 에서 동결되었습니다.
> 후속 개발은 [atelier](../atelier/) 에서 진행됩니다.
> 마이그레이션: `plugins/atelier/README.md` 참조.
```

### B.3 bumpversion 자동 범프 제외

`release.yml` 은 `bumpversion --base=HEAD~1` 로 변경된 plugin 을 자동 감지해 범프한다 (확인됨).
frozen plugin 에 README 배지 추가 같은 변경이 들어가면 **원치 않는 버전 범프**가 발생할 수 있다.

대응 (둘 다 적용):

1. **배지 추가 PR 은 `docs` type** 사용 → 버전 범프 트리거 안 됨 (git-workflow.md 표).
2. **bumpversion 에 freeze 제외 목록 추가** — `tools/bumpversion` 의 변경 감지에서 frozen 6개를 영구 제외 (04 §4 에서 구현). 향후 실수로 frozen 경로를 건드려도 범프되지 않도록 안전망.

### B.4 frozen plugin 의 예외적 변경

frozen 은 \"개발 중단\"이지 \"불변\"이 아니다. 다음만 허용:

| 허용 변경 | type | 게이트 통과 방법 |
|---|---|---|
| README ❄️ 배지 추가 | `docs` | 1회성, freeze 적용 PR |
| 보안 패치 (긴급) | `fix` | 라벨 `freeze-exception` 부착 시 CI 게이트 우회 |

그 외 기능 변경은 **반드시 atelier 에서** 한다. CI 게이트(04 §3)가 차단.

---

## C. 롤백 시나리오

atelier 에 치명적 문제가 발견되면:

1. atelier marketplace entry 를 `deprecated: true` 로 (또는 entry 제거).
2. frozen 6개의 `deprecated` 플래그 제거 → 다시 정식 active.
3. 사용자는 `/atelier:setup` 의 역방향이 없으므로, 각 plugin 의 기존 setup 재실행으로 hook 경로 복구. → **이 역방향 절차를 atelier README 에 명시**.

> 롤백 가능성을 위해 freeze 단계에서 6개 디렉토리를 **삭제하지 않는** 것이 결정적으로 중요 (00 §4.1 과 일치).

---

## D. 다음 단계

04-rollout.md 에서 위 마이그레이션/freeze 운영을 실제 PR 로 어떻게 쪼개고,
CI 게이트를 어떻게 구현하며, 검증을 어떻게 수행하는지 정리.
