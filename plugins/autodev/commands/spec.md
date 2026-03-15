---
description: 스펙 관리 — list, status, pause, resume
argument-hint: "<action> [id]"
allowed-tools: ["Bash"]
---

# 스펙 관리 (/spec)

스펙 목록 조회, 진행도 확인, 일시정지/재개를 수행합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/spec list [--repo <name>]` — 스펙 목록
- `/spec status <id>` — 스펙 진행도 상세
- `/spec pause <id>` — 스펙 일시정지
- `/spec resume <id>` — 스펙 재개

## 실행

인자에 따라 적절한 CLI 명령을 실행합니다.

### list

```bash
autodev spec list --json [--repo <name>]
```

결과를 테이블 형식으로 출력합니다:

```
📋 등록된 스펙:

  ID          레포           제목                상태      진행도
  auth-v2     org/repo-a     Auth Module v2      Active    3/5 (60%)
  payment     org/repo-b     Payment Gateway     Active    1/4 (25%)
  refund      org/repo-b     Refund Service      Paused    0/3 (0%)
```

### status

```bash
autodev spec status <id> --json
```

진행도를 이슈 단위로 상세 출력합니다:

```
📋 Auth Module v2 (auth-v2)
  상태: Active | 진행도: 3/5 (60%)

  ✅ #42 JWT middleware (done)
  ✅ #43 Token API (done)
  🔄 #44 Session adapter (implementing)
  ⏳ #45 Error handling (pending)
  🔍 #46 Missing tests (gap, analyzing)

  Acceptance Criteria:
  ✅ POST /auth/login → JWT 반환 (200)
  ✅ 만료 토큰 → 401 반환
  ⬜ POST /auth/refresh → 새 토큰 반환
  ⬜ cargo test -p auth 전체 통과
```

### pause

```bash
autodev spec pause <id>
```

일시정지 결과를 출력합니다.

### resume

```bash
autodev spec resume <id>
```

재개 결과를 출력합니다. Claw가 다음 틱에서 재판단함을 안내합니다.
