---
description: 지정한 브랜치(또는 기본 브랜치)로 전환 후 최신 상태로 동기화
argument-hint: "[branch] [--force]"
allowed-tools:
  - Bash
  - AskUserQuestion
---

# Git Sync

지정한 브랜치(또는 기본 브랜치)로 전환하고 원격 저장소의 최신 상태로 동기화합니다.

> 인자 파싱·stash 정책·브랜치 상태별 처리 매트릭스·결과 보고 형식은 `git` skill 의 `references/sync-strategy.md` 에 있습니다. 이 커맨드는 진입점만 담습니다.

## Context

- Current branch: !`git branch --show-current`
- Has uncommitted changes: !`git status --short`

## Usage

- `/atelier:sync` - 기본 브랜치로 동기화
- `/atelier:sync develop` - develop 브랜치로 동기화
- `/atelier:sync --force` - stash 후 기본 브랜치로 동기화
- `/atelier:sync develop --force` - stash 후 develop으로 동기화
- `/atelier:sync --force develop` - 위와 동일 (순서 무관)

## Execution

`git` skill 의 `references/sync-strategy.md` 절차를 수행합니다:

1. **인자 파싱** — `--force`/브랜치 이름 순서 무관, 미지정 시 기본 브랜치 자동 감지
2. **변경사항 처리** — `--force` 없이 변경사항 있으면 중단(exit 1), `--force` 면 stash
3. **브랜치 존재 여부 → 처리 매트릭스** — 로컬/원격 조합별 checkout 또는 tracking 생성, 둘 다 없으면 AskUserQuestion 으로 새 브랜치 생성 확인
4. **결과 보고** — 전환 결과 + stash/tracking/new 안내

## Notes

- 브랜치 미지정 시 기본 브랜치(main/master) 자동 감지
- `--force` 는 stash 만 하고 변경사항을 버리지 않음 (`git stash list` 로 확인)
- 원격에만 있는 브랜치 지정 시 자동 tracking 브랜치 생성

상세 절차·처리 매트릭스·Output Examples 는 `git` skill 의 `references/sync-strategy.md` 참조.
