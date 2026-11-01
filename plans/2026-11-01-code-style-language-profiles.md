# code-style 공통 룰을 언어 프로필로 적용한다

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-01 · 브랜치: docs/code-style-language-profiles · 이슈: 없음 · 승인 출처: 사용자 승인 (방안 B 선택, 세부는 승인 대기)

## 배경

atelier code-style 의 `common/` 룰 10개는 내용이 전부 언어와 상관없다 (예시도 의사코드). 그런데 적용 범위가 Go·Rust 에 묶여 있다.

1. **경로 고정**: 각 룰의 `paths:` 에 `**/*.go`, `*_client.rs` 같은 Go·Rust 경로를 직접 적었다. TS·Java·Kotlin 파일을 열면 로드되지 않는다.
2. **역할 파일명이 언어마다 다르다**: 테스트만 봐도 Go `*_test.go`, Java `*Test.java`, TS `*.test.ts`. 확장자만 늘려서는 역할 룰이 로드되지 않는다.
3. **설치 경로 없음**: `common/` 은 `--lang go|rust` 설치에 딸려서만 깔린다. TS·Java 프로젝트는 받을 방법이 없다.

사용자는 언어와 무관한 룰을 다른 언어에도 적용하길 원했고, 두 방안 중 B(언어 프로필 + CLI 렌더링)를 골랐다.

## 요구사항

- R1. 공통 룰 원본은 언어 경로를 모른다. 어느 역할의 파일에 적용되는지만 적는다.
- R2. 언어마다 "역할 → 경로" 를 한 곳(프로필)에 둔다. 언어를 추가할 때 공통 룰 파일을 고치지 않는다.
- R3. 설치할 때 CLI 가 프로젝트에 설치된 언어들의 프로필로 공통 룰의 `paths:` 를 채운다. 같은 입력이면 결과가 항상 같다.
- R4. 언어 룰 없이 프로필만 있는 언어(`ts`·`java`·`kotlin`)도 `--lang` 으로 고를 수 있고, 고르면 공통 룰이 설치된다.
- R5. `check`·`update` 는 지금처럼 OK·DRIFTED·NEW·ORPHANED 로 판정한다. 기준만 "원본 그대로" 에서 "렌더링 결과" 로 바뀐다.

## 결정

### 1. 역할은 닫힌 값 집합

| 역할 | 뜻 | 쓰는 공통 룰 |
|---|---|---|
| `source` | 그 언어의 모든 소스 파일 | closed-value-sets, cohesion, comments, readable-body, role-essentials, feature-flags |
| `config` | 설정 계층 파일 | config-loading |
| `boundary` | 외부 시스템과 닿는 파일 (client·adapter·repository·handler·middleware) | external-boundary, transparent-wrapping |
| `test` | 테스트 파일 | testing |

- CLI 에 enum 으로 둔다. 모르는 역할은 오류다.
- external-boundary 와 transparent-wrapping 은 지금 경로가 같으므로 `boundary` 하나로 묶는다.

### 2. 공통 룰 원본 frontmatter

```yaml
---
name: testing
description: ...
roles: [test]
---
```

- `paths:` 대신 `roles:` 를 쓴다. 공통 룰 원본에 `paths:` 가 있거나 `roles:` 가 없으면 CLI 가 오류를 낸다.
- 렌더링은 `roles:` 줄만 `paths:` 목록으로 바꾸고 나머지 바이트는 그대로 둔다.

### 3. 언어 프로필

`rules/code-style/<lang>/profile.json`:

```json
{
  "roles": {
    "source":   ["**/*.java"],
    "config":   ["**/*Config.java", "**/*Properties.java", "**/config/**/*.java"],
    "boundary": ["**/*Client.java", "**/*Adapter.java", "**/*Repository.java", "**/*Controller.java", "**/*Handler.java", "**/*Interceptor.java", "**/*Filter.java"],
    "test":     ["**/src/test/**/*.java", "**/*Test.java"]
  }
}
```

- JSON 으로 둔다. CLI 에 이미 `serde_json` 이 있고, YAML·TOML 크레이트를 새로 들이지 않는다.
- 네 역할을 모두 적어야 한다. 빠진 역할과 모르는 역할은 오류다.
- 중괄호 glob(`*.{ts,tsx}`)은 쓰지 않고 펼쳐 적는다. Claude Code `paths:` 의 중괄호 지원을 확인하지 않았다.
- `go`·`rust` 프로필 값은 지금 공통 룰에 적힌 Go·Rust 경로를 그대로 옮긴다.
- `ts`·`java`·`kotlin` 의 경로 초안은 구현 PR 에서 사용자 리뷰로 확정한다.

### 4. 렌더링 규칙 (결정적 변환)

- 대상 언어 = 프로젝트에 설치된 언어 ∪ 이번에 `--lang` 으로 고른 언어. `update --lang go` 처럼 일부만 골라도 공통 룰은 설치된 언어 전체로 렌더링한다. 그러지 않으면 일부 언어 갱신이 다른 언어의 경로를 지운다.
- 경로 순서: 언어 이름순 → 룰의 `roles` 순서 → 프로필에 적힌 순서. 중복은 첫 번째만 남긴다.
- 렌더링 결과 `paths:` 가 비면 오류다. Claude Code 에서 `paths:` 가 없는 룰은 모든 파일에 상시 로드되기 때문이다.
- 언어 룰(`go/*.md`, `rust/*.md`)은 렌더링하지 않고 지금처럼 그대로 복사한다.

### 5. 설치된 언어 표시

- 프로필만 있는 언어는 복사할 `.md` 가 없어 프로젝트에 흔적이 남지 않는다. 그러면 `update`·`check` 가 설치 여부를 모른다.
- 그래서 `profile.json` 도 언어 디렉토리의 관리 대상 파일로 그대로 복사한다. 설치 여부 판단(언어 디렉토리 존재)은 지금 로직을 그대로 쓴다.
- `.claude/rules/` 아래의 JSON 은 Claude Code 가 룰로 로드하지 않는다.

### 6. 판정 (check / update)

- 언어 디렉토리 파일(`.md`, `profile.json`): 지금처럼 원본과 그대로 비교한다.
- `common/` 파일: 렌더링 결과와 비교한다. drift 모듈의 `check_verbatim_copy` 는 경로를 받아 원본을 읽으므로, "기대 내용 문자열" 을 받는 비교를 code_style 쪽에 둔다.
- 언어를 추가 설치하면 공통 룰의 기대 내용이 바뀌어 DRIFTED 가 된다. install 은 지금처럼 백업 후 덮어쓴다.

### 7. 코드 배치 (CLI)

- `code_style/core/role.rs`: `Role` enum 과 문자열 변환.
- `code_style/core/profile.rs`: `profile.json` 파싱·검증.
- `code_style/core/render.rs`: `render_common(source, profiles) -> Result<String, String>` 순수 함수.
- `code_style/commands/mod.rs`: `judge` 가 common 그룹이면 렌더링 결과를 기대 내용으로 쓰고, `apply` 가 그 내용을 쓴다. `rule_files` 는 `profile.json` 도 포함한다.

## 사이드이펙트

| 대상 | 영향 | 처리 |
|---|---|---|
| 이미 설치한 프로젝트 | Go 만 설치한 프로젝트의 공통 룰에서 `.rs` 경로가 빠지는 등 기대 내용이 바뀐다. 첫 `update` 에서 공통 룰이 DRIFTED 로 나온다 | 정상 경로다. `/atelier:code-style update` 가 확인 후 백업하고 갱신한다. 릴리스 노트에 적는다 |
| 프로젝트가 고친 공통 룰 | 언어 추가 설치 때 백업 후 덮어써진다 | 지금 install 동작과 같다. 아래 "정할 것" 3 참고 |
| `commands/code-style.md` | 언어 목록(`[go]` `[rust]`), common 설치 설명, 원본 → 설치 위치 표 | 함께 고친다 |
| `commands/setup.md`, `skills/workflow/SKILL.md`, `README.md` | "언어별 코드 스타일 룰(Go·Rust)" 문구 | 언어 목록을 빼고 "언어별" 로만 적는다 |
| `commands/update.md` | `code-style check`·`update` 호출만 한다 | 변경 없음 |
| drift 모듈 | `check_verbatim_copy` 공유 | 수정하지 않는다. 렌더링 비교는 code_style 에 둔다 |
| 테스트 (`tests/code_style_*.rs`) | MemFs 픽스처의 common 원본이 `paths:` 없는 본문이다 | 픽스처에 `roles:` frontmatter 와 `profile.json` 을 추가한다 |

## 버린 대안

| 대안 | 이유 |
|---|---|
| A. 공통 룰마다 모든 언어 경로를 직접 나열 | 언어를 하나 추가할 때마다 공통 룰 10개를 고쳐야 한다. 역할 파일명이 언어마다 달라 파일마다 긴 목록이 생긴다. 설치 경로 문제도 따로 남는다 |
| 경로를 `**/*` 로 넓히기 | `.md`·설정 파일까지 로드돼 컨텍스트를 낭비한다 |
| `--lang` 없이 common 만 설치하는 옵션 | 역할 파일명 문제(배경 2)를 풀지 못한다 |
| 설치 후 프로젝트 사본의 `paths:` 를 사용자가 직접 고치기 | 매 `update` 마다 DRIFTED 가 되어 동기화가 깨진다 |

## 구현 순서

1. (별도 PR) feature-flags 룰을 Go·Rust 경로로 먼저 머지한다.
2. 이 plan 을 `docs` 커밋으로 남긴다.
3. 테스트 먼저 (fail): role·profile 파싱 오류, 렌더링 결정성·순서·중복 제거·빈 paths 오류, 프로필만 있는 언어 설치·감지, 언어 추가 시 common DRIFTED → update 후 OK, `update --lang` 일부 지정 시 설치 언어 전체로 렌더링.
4. 코어 구현 (pass) → `judge`·`apply` 연결.
5. 원본 이전: `go`·`rust` `profile.json` 추가, 공통 룰 10개 `paths:` → `roles:`.
6. `ts`·`java`·`kotlin` 프로필 추가.
7. 커맨드·README 문구 갱신. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` 통과.

3~7 은 PR 하나(`feat(atelier): ...`)로 낸다.

## 정할 것 (승인 대기)

1. TS 와 JS 를 프로필 하나(`ts`)로 묶을까, 나눌까. 초안은 하나로 묶는다.
2. Java 와 Kotlin 을 따로(`java`, `kotlin`) 둘까, `jvm` 하나로 묶을까. 초안은 따로 둔다 (테스트·설정 파일명 관례가 조금 다르다).
3. 이미 고친 공통 룰이 있는 프로젝트에 언어를 추가 설치할 때 확인을 받을까. 초안은 `/atelier:code-style` 커맨드가 install 전에 `check` 를 돌리고, DRIFTED·ORPHANED 가 있으면 update 와 같은 질문을 하는 것이다. CLI 는 바꾸지 않는다.
