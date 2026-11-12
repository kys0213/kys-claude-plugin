# Rust CLI 레이어를 `commands/` 디렉토리로 정하기

> **Plan — 이 시점의 결정 기록.** 현재 정책의 신뢰 소스가 아니다.
> 날짜: 2026-11-12 · 브랜치: claude/magical-lovelace-bnfj4j (PR kys0213/kys-claude-plugin#966) · 승인 출처: 사용자 승인 (2026-11-12 "commands 로 패키지 이름을 하면 되는거 아니야?", "이 PR(#966)에 추가", "commands/ 로 이동")
> 선행 기록: `plans/2026-11-12-atelier-kanban-boyscout-cleanup.md`

## 배경

`rust-cli.md` 의 `**/cli/**/*.rs` 가 크레이트 디렉토리 이름(`cli/`)에 걸려, 같은 크레이트의 `core/` 파일에도 CLI 규칙이 로드됐다. 보이스카웃 정리 중 core 파일 안 `mod tests` 를 어느 레이어 규칙으로 볼지 모호했다.

## 결정

- 레이어는 크레이트 이름이 아니라 디렉토리로 정한다. `rust-cli.md` 의 경로는 `**/commands/**/*.rs`, `**/cli.rs`. 쓰이지 않던 `**/cmd/**` 는 뺐다. 기존 코드가 모두 `commands/` 를 쓰므로 이름을 바꾸지 않는다.
- 인자 정의·서브커맨드 분기·결과 포매팅은 `commands/` 에 둔다. 도메인 `mod.rs`·`main.rs` 는 모듈 선언과 진입점만 둔다.
- `cli/tests/` 는 더 이상 `rust-cli.md` 에 걸리지 않으므로, 블랙박스 검증 문장을 `rust-test.md` 로 옮겼다.

## 순서

1. 규칙 변경 (메인)
2. 병렬 이동 — 동작·공개 진입점 불변, `refactor` 커밋
   - atelier kanban
   - atelier git·code_style·drift
   - atelier orchestrator·session, suggest-workflow `main.rs`
3. 게이트(reviewer·규칙 준수) 뒤 통합 검증, push, PR 설명 갱신
