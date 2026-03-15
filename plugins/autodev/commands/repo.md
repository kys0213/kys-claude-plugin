---
description: 레포 목록/상세 조회
argument-hint: "<action> [name]"
allowed-tools: ["Bash"]
---

# 레포 관리 (/repo)

등록된 레포의 목록과 상세 정보를 조회합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/repo list` — 등록된 레포 목록
- `/repo show <name>` — 레포 상세 정보

## 실행

### list

```bash
autodev repo list --json
```

결과를 테이블 형식으로 출력합니다:

```
📦 등록된 레포:

  이름            URL                                  스펙    상태
  org/repo-a      https://github.com/org/repo-a       2개     Active
  org/repo-b      https://github.com/org/repo-b       1개     Active
  org/repo-c      https://github.com/org/repo-c       없음    Issue 모드
```

### show

```bash
autodev repo show <name> --json
```

레포 상세 정보를 출력합니다:

```
📦 org/repo-a

  URL: https://github.com/org/repo-a
  로컬 경로: /Users/me/repos/repo-a
  기본 브랜치: main

  설정:
    스캔 주기: 300초
    이슈 동시 처리: 2
    PR 동시 처리: 2
    감시 대상: Issues, Pull Requests

  스펙:
    auth-v2     Auth Module v2      Active  3/5 (60%)
    cache       Cache Layer         Paused  0/2 (0%)

  최근 활동:
    10분 전  #43 Token API → done
    25분 전  #44 Session adapter → implementing
```
