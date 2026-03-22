---
description: 스펙 관리 — add, update, list, status, pause, resume, remove
argument-hint: "<subcommand> [args]"
allowed-tools: ["AskUserQuestion", "Bash", "Read", "Glob", "Grep"]
---

# 스펙 관리 (/spec)

스펙의 등록, 수정, 목록 조회, 진행도 확인, 일시정지/재개를 하나의 진입점에서 수행합니다.

> v4에서 `/add-spec`, `/update-spec`, `/spec`으로 분리되어 있던 기능을 통합합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

| 서브커맨드 | 설명 | v4 대응 |
|-----------|------|---------|
| `/spec add [file]` | 대화형 검증 + 보완 후 스펙 등록 | `/add-spec` |
| `/spec update <id>` | 대화형 영향 분석 후 스펙 수정 | `/update-spec` |
| `/spec list [--repo <name>]` | 스펙 목록 | `/spec list` |
| `/spec status <id>` | 스펙 진행도 상세 | `/spec status` |
| `/spec pause <id>` | 스펙 일시정지 | `/spec pause` |
| `/spec resume <id>` | 스펙 재개 | `/spec resume` |
| `/spec remove <id>` | 스펙 제거 | (신규) |

## 실행

### Step 0: CLI 바이너리 확인

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

실패 시 바이너리 설치/업데이트를 안내하고 중단합니다.

### Step 1: 서브커맨드 분기

인자를 파싱하여 아래 섹션으로 분기합니다. 인자가 없으면 `list`를 실행합니다.

---

## add

레포의 Claude 세션에서 실행합니다. 코드베이스를 분석하여 스펙을 검증하고, 누락된 섹션을 대화형으로 보완합니다.

### 사용법

- `/spec add ./SPEC.md` — 파일 기반 스펙 등록
- `/spec add` — 대화형으로 스펙 작성

### Add Step 1: 레포 컨텍스트 분석

현재 레포의 컨텍스트를 수집합니다:

1. **git remote에서 레포 이름 추출**:
   ```bash
   git remote get-url origin
   ```

2. **코드베이스 구조 스캔** — Glob으로 주요 파일 패턴 탐색:
   - 언어/프레임워크 감지 (Cargo.toml, package.json, go.mod 등)
   - 테스트 디렉토리 존재 여부 (tests/, __tests__/, *_test.go 등)
   - 기존 설정 파일 (.claude/rules/, docker-compose.yml 등)

3. **autodev 등록 여부 확인**:
   ```bash
   autodev repo list --json
   ```
   현재 레포가 미등록이면 `/auto setup`을 먼저 실행하라고 안내합니다.

### Add Step 2: 스펙 파싱 + 구조 검증

인자로 파일이 주어진 경우 Read로 파일 내용을 읽습니다.
인자가 없으면 AskUserQuestion으로 어떤 기능을 구현하고 싶은지 물어보고 레포 분석 결과를 기반으로 스펙 초안을 작성합니다.

**5개 필수 섹션을 검증합니다**:

| # | 섹션 | 검증 기준 |
|---|------|----------|
| 1 | 요구사항 (Requirements) | 구체적인 기능 목록이 존재하는지 |
| 2 | 아키텍처/컴포넌트 | 모듈/인터페이스 정의가 존재하는지 |
| 3 | 기술 스택 + 컨벤션 | 언어/프레임워크가 명시되어 있는지 |
| 4 | 테스트 환경 구성 | 실행 가능한 테스트 명령이 존재하는지 |
| 5 | Acceptance Criteria | 검증 가능한 완료 조건이 존재하는지 |

검증 결과를 다음 형식으로 출력합니다:

```
스펙 검증 결과:

  요구사항 — N개 기능 정의됨
  아키텍처 — N개 모듈 정의됨
  기술 스택 — 감지됨, 컨벤션 미명시
  테스트 환경 — 누락
  Acceptance Criteria — 누락
```

### Add Step 3: 누락 섹션 대화형 보완

누락되거나 불완전한 섹션에 대해 AskUserQuestion으로 순차 보완합니다.

각 섹션에 대해:
1. Add Step 1에서 수집한 레포 컨텍스트를 기반으로 자동 구성을 제안
2. 사용자가 "제안대로" 또는 "직접 작성"을 선택
3. 직접 작성 시 사용자 입력을 반영하여 섹션 완성

모든 필수 섹션이 채워지면 최종 스펙 내용을 요약하여 확인받습니다.

### Add Step 4: Claw 활성화 확인

레포 설정에서 Claw가 비활성화되어 있는지 확인합니다:

```bash
autodev repo show <name> --json
```

Claw가 비활성화 상태면 AskUserQuestion으로 활성화 여부를 묻습니다.

### Add Step 5: 스펙 등록

검증 완료된 스펙을 임시 파일에 저장하고 CLI로 등록합니다:

```bash
SPEC_TMP=$(mktemp /tmp/autodev-spec-XXXXXX.md)
# ... 검증 완료된 스펙을 $SPEC_TMP에 저장 ...
autodev spec add --title "<title>" --file "$SPEC_TMP" --repo <repo-name>
rm -f "$SPEC_TMP"
```

등록 성공 시:

```
스펙 등록 완료!

  제목: <title>
  요구사항: N개 기능
  아키텍처: N개 모듈
  테스트 환경: unit + integration + e2e
  Acceptance Criteria: N개 항목

Claw가 다음 틱에서 이슈를 분해합니다.
```

---

## update

레포의 Claude 세션에서 실행합니다. 기존 스펙의 내용을 대화형으로 수정하고, 변경 영향을 분석합니다.

### 사용법

- `/spec update <spec-id>` — 지정 스펙 수정

### Update Step 1: 현재 스펙 + 진행 상태 로드

```bash
autodev spec status <spec-id> --json
```

현재 상태를 요약하여 출력합니다:

```
<title> 현재 상태:
  진행도: N/M (X%)
  #42 JWT middleware (done)
  #44 Session adapter (implementing)
  #45 Error handling (pending)
```

### Update Step 2: 변경 방향 파악

AskUserQuestion으로 어떤 부분을 변경하고 싶은지 물어봅니다.
사용자의 응답을 기반으로 변경 의도를 파악합니다.

### Update Step 3: 변경 영향 분석

코드베이스와 기존 이슈 상태를 대조하여 영향 범위를 분석합니다:

1. **이슈 영향**: 불필요해지는 이슈, 수정 필요한 이슈 식별
2. **코드 영향**: Glob/Grep으로 관련 파일 탐색
3. **Acceptance Criteria 영향**: 제거/추가/수정 항목 식별

분석 결과를 출력합니다:

```
영향 범위:
  - #44 Session adapter (implementing) → 불필요해짐
  - #43 Token API (done) → refresh 로직 수정 필요
  - 아키텍처: auth/session.rs 제거, auth/token.rs 수정
  - Acceptance Criteria: 세션 관련 항목 제거, stateless 검증 추가
```

### Update Step 4: 변경 제안 + 사용자 승인

제안 사항을 정리하여 AskUserQuestion으로 확인받습니다:

```
제안:
  1. #44 → autodev:skip (불필요)
  2. #43 → 새 이슈로 재작업
  3. Acceptance Criteria 업데이트
  4. 아키텍처 섹션 수정

이대로 진행할까요?
```

### Update Step 5: 스펙 업데이트 실행

승인 후 수정된 스펙을 임시 파일에 저장하고 CLI로 업데이트합니다:

```bash
SPEC_TMP=$(mktemp /tmp/autodev-spec-XXXXXX.md)
# ... 수정된 스펙을 $SPEC_TMP에 저장 ...
autodev spec update <spec-id> --file "$SPEC_TMP"
rm -f "$SPEC_TMP"
```

skip 처리가 필요한 이슈가 있으면:

```bash
autodev queue skip <work-id>
```

완료 메시지:

```
스펙 업데이트 완료.
  - 변경 사항 요약
  - Claw가 다음 틱에서 업데이트된 스펙 기반으로 재판단합니다.
```

---

## list

스펙 목록을 조회합니다.

```bash
autodev spec list --json [--repo <name>]
```

결과를 테이블 형식으로 출력합니다:

```
등록된 스펙:

  ID          레포           제목                상태      진행도
  auth-v2     org/repo-a     Auth Module v2      Active    3/5 (60%)
  payment     org/repo-b     Payment Gateway     Active    1/4 (25%)
  refund      org/repo-b     Refund Service      Paused    0/3 (0%)
```

---

## status

스펙의 진행도를 이슈 단위로 상세 출력합니다.

```bash
autodev spec status <id> --json
```

```
Auth Module v2 (auth-v2)
  상태: Active | 진행도: 3/5 (60%)

  #42 JWT middleware (done)
  #43 Token API (done)
  #44 Session adapter (implementing)
  #45 Error handling (pending)
  #46 Missing tests (gap, analyzing)

  Acceptance Criteria:
  POST /auth/login → JWT 반환 (200)
  만료 토큰 → 401 반환
  POST /auth/refresh → 새 토큰 반환
  cargo test -p auth 전체 통과
```

---

## pause

스펙을 일시정지합니다.

```bash
autodev spec pause <id>
```

일시정지 결과를 출력합니다.

---

## resume

스펙을 재개합니다.

```bash
autodev spec resume <id>
```

재개 결과를 출력합니다. Claw가 다음 틱에서 재판단함을 안내합니다.

---

## remove

스펙을 제거합니다.

```bash
autodev spec remove <id>
```

제거 전 AskUserQuestion으로 확인합니다.
