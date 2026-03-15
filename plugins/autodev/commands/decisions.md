---
description: Claw 판단 이력 조회
argument-hint: "[repo]"
allowed-tools: ["Bash"]
---

# Claw 판단 이력 (/decisions)

Claw가 수행한 판단(advance, skip, HITL 요청, 스펙 분해 등)의 이력을 조회합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/decisions` — 전체 판단 이력 (최근 20건)
- `/decisions <repo>` — 특정 레포의 판단 이력

## 실행

### Step 1: 판단 이력 조회

```bash
autodev spec decisions --json -n 20
```

인자로 레포가 주어진 경우 JSON 결과에서 해당 레포의 항목만 필터링합니다.

### Step 2: 이력 출력

시간순으로 정렬하여 출력합니다:

```
🧠 Claw 판단 이력 (최근 20건):

  시각              레포           동작        근거
  03-15 14:30      org/repo-a     advance     #44 구현 완료, 리뷰 단계로 전이
  03-15 14:25      org/repo-a     decompose   Auth v2 스펙 → 이슈 5개 분해
  03-15 14:20      org/repo-b     hitl        #51 리뷰 3회 반복, 사람 확인 필요
  03-15 14:15      org/repo-a     skip        #43 세션 어댑터 불필요 (스펙 변경)
  ...
```

항목이 없으면 "판단 이력이 없습니다."를 출력합니다.
