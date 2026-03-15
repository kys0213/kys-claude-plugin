---
description: 스펙 등록 — 대화형 검증 + 보완 후 autodev에 등록
argument-hint: "[file]"
allowed-tools: ["AskUserQuestion", "Bash", "Read", "Glob", "Grep"]
---

# 스펙 등록 (/add-spec)

레포의 Claude 세션에서 실행합니다. 코드베이스를 분석하여 스펙을 검증하고, 누락된 섹션을 대화형으로 보완합니다.

> `cli-reference` 스킬을 참조하여 autodev CLI 명령을 호출합니다.

## 사용법

- `/add-spec ./SPEC.md` — 파일 기반 스펙 등록
- `/add-spec` — 대화형으로 스펙 작성

## 실행

### Step 0: CLI 바이너리 확인

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

실패 시 바이너리 설치/업데이트를 안내하고 중단합니다.

### Step 1: 레포 컨텍스트 분석

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
   현재 레포가 미등록이면 `/auto-setup`을 먼저 실행하라고 안내합니다.

### Step 2: 스펙 파싱 + 구조 검증

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
🔍 스펙 검증 결과:

✅ 요구사항 — N개 기능 정의됨
✅ 아키텍처 — N개 모듈 정의됨
⚠️ 기술 스택 — 감지됨, 컨벤션 미명시
❌ 테스트 환경 — 누락
❌ Acceptance Criteria — 누락
```

### Step 3: 누락 섹션 대화형 보완

누락되거나 불완전한 섹션에 대해 AskUserQuestion으로 순차 보완합니다.

각 섹션에 대해:
1. Step 1에서 수집한 레포 컨텍스트를 기반으로 자동 구성을 제안
2. 사용자가 "제안대로" 또는 "직접 작성"을 선택
3. 직접 작성 시 사용자 입력을 반영하여 섹션 완성

모든 필수 섹션이 채워지면 최종 스펙 내용을 요약하여 확인받습니다.

### Step 4: Claw 활성화 확인

레포 설정에서 Claw가 비활성화되어 있는지 확인합니다:

```bash
autodev repo show <name> --json
```

Claw가 비활성화 상태면 AskUserQuestion으로 활성화 여부를 묻습니다.

### Step 5: 스펙 등록

검증 완료된 스펙을 임시 파일에 저장하고 CLI로 등록합니다:

```bash
SPEC_TMP=$(mktemp /tmp/autodev-spec-XXXXXX.md)
# ... 검증 완료된 스펙을 $SPEC_TMP에 저장 ...
autodev spec add --title "<title>" --file "$SPEC_TMP" --repo <repo-name>
rm -f "$SPEC_TMP"
```

등록 성공 시:

```
✅ 스펙 등록 완료!

  제목: <title>
  요구사항: N개 기능
  아키텍처: N개 모듈
  테스트 환경: unit + integration + e2e
  Acceptance Criteria: N개 항목

Claw가 다음 틱에서 이슈를 분해합니다.
```
