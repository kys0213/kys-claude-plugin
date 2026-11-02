# code-style 공통 룰을 언어 프로필로 적용한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-01 · 브랜치: feat/code-style-language-profiles · 이슈: 없음 · 승인 출처: 사용자 승인

## 배경

atelier code-style 의 `common/` 룰은 내용이 전부 언어와 상관없다 (예시도 의사코드). 그런데 적용 범위가 Go·Rust 에 묶여 있다.

1. **경로 고정**: 각 룰의 `paths:` 에 `**/*.go`, `*_client.rs` 같은 Go·Rust 경로를 직접 적었다. TS·Java·Kotlin 파일을 열면 로드되지 않는다.
2. **역할 파일명이 언어마다 다르다**: 테스트만 봐도 Go `*_test.go`, Java `*Test.java`, TS `*.test.ts`. 확장자만 늘려서는 역할 룰이 로드되지 않는다.
3. **설치 경로 없음**: `common/` 은 `--lang go|rust` 설치에 딸려서만 깔린다.
4. **룰 단위 제어 없음**: 설치된 룰 파일을 지워도 다음 `update` 가 NEW 로 보고 다시 설치한다.

사용자 요구: 언어 무관 원칙과 언어별 원칙을 디렉토리로 나눠, **에이전트와 사용자 모두 쉽게 제어하고 어떤 룰이 설치됐는지 바로 알 수 있게** 한다. 프로젝트에는 고른 언어에 맞게 이렇게 설치된다.

```
<project>/.claude/rules/atelier/code-style/
  common/          # 언어 무관 원칙 — paths 는 설치된 언어 기준으로 채움
  go/              # 언어별 원칙
  installed.json   # 설치 기록
```

## 결정

### 1. 원본 구조

```
rules/code-style/
  common/*.md            # frontmatter 에 paths 대신 roles
  <lang>/profile.json    # 역할 → 경로 (언어마다 필수)
  <lang>/*.md            # 언어별 룰 (그대로 복사)
```

- 공통 룰은 평면으로 둔다. 역할별 하위 디렉토리(`common/test/` 등)는 설치 구조가 깊어져 한눈에 안 보이므로 버렸다 (사용자 의견).
- 언어마다 파일명 규칙 룰(`<lang>-file-naming.md`)을 둔다. 역할 룰이 로드되는 파일명을 그 언어 파일에서 알려 준다. 새로 추가하는 `ts`·`java`·`kotlin` 은 이 룰 하나와 프로필만 둔다.

### 2. 역할은 닫힌 값 집합

| 역할 | 뜻 | 쓰는 공통 룰 |
|---|---|---|
| `source` | 그 언어의 모든 소스 파일 | closed-value-sets, cohesion, comments, readable-body, role-essentials, feature-flags |
| `config` | 설정 계층 파일 | config-loading |
| `boundary` | 외부 시스템과 닿는 파일 | external-boundary, transparent-wrapping |
| `test` | 테스트 파일 | testing |

- 공통 룰 frontmatter: `roles: [test]`. `paths:` 가 있거나 `roles:` 가 없으면 오류.
- 프로필: `{"roles": {"source": [...], "config": [...], "boundary": [...], "test": [...]}}`. 네 역할을 모두 적고, 모르는 키는 오류. JSON 인 이유: CLI 에 `serde_json` 이 이미 있다.
- 중괄호 glob 은 쓰지 않고 펼쳐 적는다 (Claude Code `paths:` 의 중괄호 지원을 확인하지 않았다).

### 3. 렌더링 (결정적 변환)

- 대상 언어 = 설치된 언어(플러그인에 원본이 있는 것) ∪ 이번에 `--lang` 으로 고른 언어. 일부 언어만 갱신해도 공통 룰은 설치된 언어 전체로 렌더링한다.
- `roles:` 한 줄을 `paths:` 목록으로 바꾸고 나머지 바이트는 그대로 둔다.
- 경로 순서: 언어 이름순 → 룰의 `roles` 순서 → 프로필 순서. 중복은 첫 번째만 남긴다.
- 결과 `paths:` 가 비면 오류. `paths:` 없는 룰은 모든 파일에 상시 로드되기 때문이다.
- 언어 룰은 렌더링하지 않고 그대로 복사한다. `profile.json` 은 프로젝트에 복사하지 않는다.

### 4. 설치 기록 `installed.json`

```json
{
  "langs": ["go"],
  "rules": ["common/cohesion.md", "go/go-errors.md"]
}
```

- 설치된 언어는 기록의 `langs` 로 정한다. 기록이 없으면(이전 버전 설치본) 지금처럼 언어 디렉토리로 정한다.
- 갱신한 그룹의 `rules` = 그 그룹의 현재 원본 파일 전부. 일부러 뺀 룰도 기록에 남아 "뺐다"를 기억한다.
- 내용이 바뀔 때만 쓴다.

### 5. 판정

| STATUS | 조건 | 처리 (install·update) | exit |
|---|---|---|---|
| `OK` | 사본 = 기대 내용 | 없음 | 0 |
| `DRIFTED` | 사본 ≠ 기대 내용 | 백업 후 덮어씀 | 1 |
| `NEW` | 원본 있음, 사본 없음, 기록에 없음 | 설치 | 1 |
| `EXCLUDED` | 원본 있음, 사본 없음, 기록에 있음 | 없음 (사용자가 뺀 룰) | 0 |
| `ORPHANED` | 사본 있음, 원본 없음 | 백업 후 삭제 | 1 |

- 기대 내용: 언어 룰은 원본 그대로, 공통 룰은 렌더링 결과.
- 요약 줄에 `N excluded` 를 더한다.

### 6. 제어 방법

| 하고 싶은 것 | 방법 |
|---|---|
| 언어 추가 | `install --lang ts` |
| 룰 하나 빼기 | 그 사본 파일을 지운다 |
| 뺀 룰 되살리기 | `restore --rule common/feature-flags.md` (반복 가능) |
| 언어 빼기 | `uninstall --lang go` — 그 언어 사본을 백업 후 지우고, 공통 룰을 남은 언어로 다시 렌더링. 남은 언어가 없으면 공통 룰과 기록도 백업 후 지운다 |

- 언어 디렉토리를 통째로 지우면 그 언어의 룰을 모두 뺀 것(EXCLUDED)으로 본다. 언어 자체를 빼려면 `uninstall` 을 쓴다. 디렉토리 삭제를 언어 제거로 해석하면 "룰을 전부 뺐다"와 구분할 수 없다.
- 공통 룰이 바뀌는 install(언어 추가)은 프로젝트가 고친 공통 룰을 덮어쓸 수 있다. `/atelier:code-style` 커맨드가 install 전에 `check` 를 돌려 DRIFTED·ORPHANED 가 있으면 update 와 같은 확인을 받는다. CLI 는 묻지 않는다.

### 7. 언어 초안

- `ts`: TS·JS 를 프로필 하나로 묶는다 (`.ts` `.tsx` `.mts` `.cts` `.js` `.jsx` `.mjs` `.cjs`).
- `java`, `kotlin`: 따로 둔다. 테스트·설정 파일명 관례가 조금 다르다.
- `go`·`rust` 프로필은 지금 공통 룰에 적힌 경로를 그대로 옮긴다.

### 8. 코드 배치 (CLI)

- `code_style/core/role.rs`, `profile.rs`, `render.rs`, `record.rs`: 순수 파싱·변환.
- `code_style/commands/`: `install`·`update`·`check` 에 `restore`·`uninstall` 추가. 판정·적용은 공통 모듈에서 기대 내용 문자열로 비교한다. drift 모듈의 `check_verbatim_copy` 는 경로를 받아 원본을 읽기 때문에 쓰지 않는다.

## 사이드이펙트

| 대상 | 영향 | 처리 |
|---|---|---|
| 이미 설치한 프로젝트 | Go 만 설치했으면 공통 룰에서 `.rs` 경로가 빠진다. 첫 `update` 에서 공통 룰이 DRIFTED 로 나오고 기록 파일이 생긴다 | 정상 경로. 확인 후 백업·갱신 |
| `check` 출력 형식 | `EXCLUDED` 토큰, 요약 줄에 `N excluded` | 커맨드 문서 갱신. `/atelier:update` 는 exit 코드만 보므로 영향 없음 |
| `commands/code-style.md` | 언어 목록, 원본 → 설치 위치 표, STATUS 표, 제어 방법 | 함께 고친다 |
| `commands/setup.md`, `skills/workflow/SKILL.md`, `README.md` | "언어별 코드 스타일 룰(Go·Rust)" 문구 | 언어 목록을 뺀다 |
| drift 모듈 | 수정 없음 | — |
| 테스트 | 픽스처 common 원본에 `roles:`, 언어마다 `profile.json` 필요 | 픽스처 갱신 |

## 버린 대안

| 대안 | 이유 |
|---|---|
| 공통 룰마다 모든 언어 경로를 직접 나열 | 언어를 추가할 때마다 공통 룰을 전부 고쳐야 하고, 역할 파일명이 언어마다 달라 목록이 길어진다 |
| 역할별 하위 디렉토리 (`common/test/`) | 설치 구조가 깊어져 설치된 룰을 한눈에 보기 어렵다 |
| `profile.json` 을 설치 표시로 복사 | 설치 기록이 언어와 뺀 룰을 함께 담으므로 필요 없다 |
| 언어 디렉토리 삭제 = 언어 제거 | "그 언어의 룰을 전부 뺐다"와 구분이 안 된다 |
| 설치 위치를 `.claude/rules/code-style/` 로 | `atelier/` 가 관리 주체 표시라 유지한다 (사용자 승인) |

## 구현 순서

1. 테스트 먼저 (fail): 역할·프로필 파싱 오류, 렌더링 순서·중복·빈 paths, 기록 기반 EXCLUDED, restore, uninstall, 언어 추가 시 공통 룰 재렌더링, 기록 없는 이전 설치본.
2. 코어 구현 (pass) → 커맨드 연결.
3. 원본 이전: `go`·`rust` 프로필, 공통 룰 `paths:` → `roles:`.
4. `ts`·`java`·`kotlin` 프로필과 파일명 규칙 룰.
5. 커맨드·README 문구. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
