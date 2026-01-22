# Team Claude 테스트 시나리오

이 문서는 team-claude 플러그인을 실제로 테스트하기 위한 시나리오를 정의합니다.

---

## 목차

1. [테스트 환경 준비](#1-테스트-환경-준비)
2. [Phase 1: 서버 테스트](#phase-1-서버-테스트)
3. [Phase 2: Architect 테스트](#phase-2-architect-테스트)
4. [Phase 3: Delegate 테스트](#phase-3-delegate-테스트)
5. [Phase 4: Merge 테스트](#phase-4-merge-테스트)
6. [Phase 5: 통합 E2E 테스트](#phase-5-통합-e2e-테스트)
7. [Edge Cases & 에러 처리](#edge-cases--에러-처리)

---

## 1. 테스트 환경 준비

### 1.1 사전 요구사항

```bash
# 필수 도구 확인
which bun     # Bun 런타임
which claude  # Claude CLI
which git     # Git
which gh      # GitHub CLI (PR 생성용)
```

### 1.2 테스트 프로젝트 생성

```bash
# 간단한 Node.js 프로젝트로 테스트
mkdir /tmp/team-claude-test
cd /tmp/team-claude-test
npm init -y
npm install --save-dev jest

# 기본 테스트 파일 생성
cat > calculator.js << 'EOF'
// TODO: 구현 필요
module.exports = {
  add: (a, b) => { throw new Error('Not implemented') },
  subtract: (a, b) => { throw new Error('Not implemented') },
};
EOF

cat > calculator.test.js << 'EOF'
const calc = require('./calculator');

test('add', () => {
  expect(calc.add(1, 2)).toBe(3);
});

test('subtract', () => {
  expect(calc.subtract(5, 3)).toBe(2);
});
EOF

# Git 초기화
git init
git add .
git commit -m "init: test project"
```

---

## Phase 1: 서버 테스트

### 1.1 서버 빌드 및 시작

**테스트 목적**: 서버가 정상적으로 빌드되고 시작되는지 확인

```bash
# 1. 서버 빌드
cd plugins/team-claude/server
bun install
bun build src/index.ts --compile --outfile /tmp/team-claude-server

# 2. 서버 시작
TEAM_CLAUDE_PORT=7890 /tmp/team-claude-server &
SERVER_PID=$!

# 3. Health check
sleep 2
curl -s http://localhost:7890/health
# 예상: {"status":"ok","timestamp":"..."}

# 4. 정리
kill $SERVER_PID
```

**예상 결과**:
- [x] 빌드 성공
- [x] 서버 시작 성공
- [x] `/health` 응답: `{"status":"ok"}`

---

### 1.2 서버 API 테스트

**테스트 목적**: 모든 API 엔드포인트가 정상 동작하는지 확인

```bash
# 서버 시작 상태에서

# 1. GET /info
curl -s http://localhost:7890/info | jq
# 예상: executors, config 정보

# 2. POST /tasks (태스크 생성)
TASK_RESPONSE=$(curl -s -X POST http://localhost:7890/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "checkpoint_id": "test-task",
    "checkpoint_name": "Test Task",
    "worktree_path": "/tmp/team-claude-test",
    "validation_command": "npm test"
  }')
echo $TASK_RESPONSE
TASK_ID=$(echo $TASK_RESPONSE | jq -r '.task_id')

# 3. GET /tasks (목록 조회)
curl -s http://localhost:7890/tasks | jq

# 4. GET /tasks/:id (상세 조회)
curl -s http://localhost:7890/tasks/$TASK_ID | jq

# 5. DELETE /tasks/:id (취소)
curl -s -X DELETE http://localhost:7890/tasks/$TASK_ID
```

**예상 결과**:
- [x] `/info`: executor 목록 반환
- [x] `POST /tasks`: 202 응답, task_id 반환
- [x] `GET /tasks`: 태스크 목록 반환
- [x] `GET /tasks/:id`: 태스크 상세 정보 반환
- [x] `DELETE /tasks/:id`: 태스크 취소 성공

---

### 1.3 Executor 테스트

**테스트 목적**: 각 Executor가 Worker를 정상 실행하는지 확인

#### iTerm Executor (macOS)
```bash
# iTerm에서 새 탭이 열리고 Claude가 실행되는지 확인
TEAM_CLAUDE_EXECUTOR=iterm /tmp/team-claude-server &

curl -X POST http://localhost:7890/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "checkpoint_id": "iterm-test",
    "checkpoint_name": "iTerm Test",
    "worktree_path": "/tmp/team-claude-test",
    "validation_command": "echo OK"
  }'

# 확인: iTerm 새 탭이 열렸는가?
```

#### Headless Executor
```bash
TEAM_CLAUDE_EXECUTOR=headless /tmp/team-claude-server &

curl -X POST http://localhost:7890/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "checkpoint_id": "headless-test",
    "checkpoint_name": "Headless Test",
    "worktree_path": "/tmp/team-claude-test",
    "validation_command": "echo OK"
  }'

# 확인: 백그라운드에서 실행되는가?
```

**예상 결과**:
- [x] iTerm: 새 탭에서 Worker 실행
- [x] Headless: 백그라운드 실행, 로그로 확인 가능

---

## Phase 2: Architect 테스트

### 2.1 새 설계 세션 시작

**테스트 목적**: `/team-claude:architect`가 정상 동작하는지 확인

```
# Claude Code에서 실행
/team-claude:architect "Calculator에 multiply, divide 기능 추가"
```

**예상 동작**:
1. 세션 ID 생성 (예: `abc12345`)
2. `.team-claude/sessions/abc12345/` 디렉토리 생성
3. 코드베이스 분석 시작
4. 사용자에게 질문 (AskUserQuestion)

**체크포인트**:
- [ ] 세션 디렉토리 생성됨
- [ ] `meta.json` 생성됨
- [ ] 코드베이스 분석 결과 표시됨
- [ ] 아키텍처 옵션 제안됨
- [ ] 사용자 선택 후 다음 단계 진행

---

### 2.2 설계 대화 진행

**테스트 목적**: 대화형 설계 루프가 정상 작동하는지 확인

**시나리오**:
```
에이전트: "아키텍처 옵션을 선택하세요"
  - 옵션 A: 기존 파일에 추가
  - 옵션 B: 새 모듈 분리

사용자: "옵션 A"

에이전트: "에러 처리 방식을 선택하세요"
  - throw Error
  - return null
  - Result 타입

사용자: "throw Error"
```

**체크포인트**:
- [ ] `AskUserQuestion` 호출됨
- [ ] 사용자 선택이 `decisions.json`에 기록됨
- [ ] 다음 질문으로 진행

---

### 2.3 Contract 생성

**테스트 목적**: Interface + Test Code가 정상 생성되는지 확인

**예상 산출물**:
```
.team-claude/sessions/abc12345/
├── contracts/
│   ├── multiply-feature/
│   │   ├── interface.ts       # multiply 함수 시그니처
│   │   └── contract.test.js   # 테스트 코드
│   └── divide-feature/
│       ├── interface.ts
│       └── contract.test.js
└── checkpoints/
    ├── multiply-feature.json
    └── divide-feature.json
```

**체크포인트**:
- [ ] `interface.ts` 생성됨
- [ ] `contract.test.js` 생성됨 (TDD - 구현 전 테스트)
- [ ] 테스트 코드가 실제로 실행 가능함
- [ ] Checkpoint JSON이 올바른 형식

---

### 2.4 Checkpoint 승인

**테스트 목적**: 사용자 승인 후 delegate 가능 상태가 되는지 확인

```
에이전트: "아래 Checkpoint로 구현을 진행할까요?"
  [Checkpoint 목록 표시]
  - 승인
  - 수정 필요

사용자: "승인"
```

**체크포인트**:
- [ ] `meta.json`의 `checkpointsApproved: true`로 변경
- [ ] "다음 단계 안내" 출력됨

---

## Phase 3: Delegate 테스트

### 3.1 단일 Checkpoint 위임

**테스트 목적**: 하나의 Checkpoint가 Worker에게 정상 위임되는지 확인

```bash
/team-claude:delegate multiply-feature
```

**예상 동작**:
1. Git Worktree 생성: `.team-claude/worktrees/multiply-feature/`
2. `CLAUDE.md` 생성
3. 서버에 태스크 등록
4. Worker 실행 (iTerm 새 탭 또는 headless)

**체크포인트**:
- [ ] Worktree 생성됨
- [ ] `CLAUDE.md` 내용이 올바름
- [ ] 서버에서 태스크 상태 "running"
- [ ] Worker가 실행됨

---

### 3.2 자동 검증 루프

**테스트 목적**: Worker 완료 후 자동 검증이 실행되는지 확인

**시나리오**:
1. Worker가 `calculator.js`에 `multiply` 구현
2. 서버가 `npm test` 실행
3. 결과에 따라:
   - 성공: 태스크 완료
   - 실패: 피드백 생성 → Worker 재시도

**체크포인트**:
- [ ] Validation 명령어 실행됨
- [ ] 성공 시: `status: completed`, `final_result: pass`
- [ ] 실패 시: `CLAUDE.md`에 피드백 추가됨
- [ ] 재시도 횟수가 증가함

---

### 3.3 피드백 루프 (재시도)

**테스트 목적**: 실패 시 자동 피드백 후 재시도가 되는지 확인

**시나리오** (의도적 실패):
```javascript
// Worker가 잘못된 구현을 했다고 가정
multiply: (a, b) => a + b  // 버그!
```

**예상 동작**:
1. `npm test` 실패
2. 서버가 피드백 생성:
   ```
   ## Iteration 1 - FAILED
   Expected 6, Received 5
   ```
3. `CLAUDE.md`에 피드백 추가
4. Worker 재실행
5. 수정 후 성공

**체크포인트**:
- [ ] 실패 출력이 피드백에 포함됨
- [ ] 2번째 시도에서 성공
- [ ] `iterations` 배열에 2개 기록됨

---

### 3.4 병렬 위임 (--all)

**테스트 목적**: 의존성 없는 Checkpoint들이 병렬 실행되는지 확인

```bash
/team-claude:delegate --session abc12345 --all
```

**예상 동작**:
```
Round 1 (병렬):
  - multiply-feature (의존성 없음)
  - divide-feature (의존성 없음)

Round 2:
  - integration-test (depends: multiply, divide)
```

**체크포인트**:
- [ ] Round 1에서 2개 Worker 동시 실행
- [ ] Round 1 완료 후 Round 2 시작
- [ ] 각 Worker가 독립적으로 PR 생성

---

### 3.5 에스컬레이션

**테스트 목적**: 최대 재시도 초과 시 에스컬레이션이 발생하는지 확인

**시나리오**:
- `max_retries: 3` 설정
- 3번 모두 실패

**예상 동작**:
```
⚠️ 에스컬레이션: multiply-feature

시도 횟수: 3/3 (최대 도달)

반복 실패 원인:
  [분석 결과]

권장 조치:
  - 설계 재검토
  - 수동 구현
```

**체크포인트**:
- [ ] `status: failed`, `final_result: fail`
- [ ] 에스컬레이션 메시지 출력
- [ ] 실패 이력이 기록됨

---

## Phase 4: Merge 테스트

### 4.1 자동 머지 (Conflict 없음)

**테스트 목적**: Conflict 없이 자동 머지가 되는지 확인

**사전 조건**:
- `multiply-feature` PR 완료
- `divide-feature` PR 완료
- 두 PR이 다른 파일을 수정

```bash
/team-claude:merge
```

**예상 동작**:
1. PR 목록 수집
2. 순차적 머지
3. 모두 자동 머지 성공
4. `epic → main` PR 생성

**체크포인트**:
- [ ] 모든 PR 자동 머지됨
- [ ] Epic 브랜치에 모든 커밋 포함
- [ ] `gh pr create` 실행됨

---

### 4.2 Conflict 해결 (Semi-Auto)

**테스트 목적**: Conflict 발생 시 사용자 확인이 요청되는지 확인

**사전 조건**:
- 두 Worker가 같은 파일을 수정

**예상 동작**:
1. 머지 시도 → Conflict 감지
2. Conflict Analysis Agent 호출
3. 분석 결과 표시
4. `AskUserQuestion` 으로 해결 방법 선택

**체크포인트**:
- [ ] Conflict 파일 목록 표시
- [ ] 분석 결과 표시 (양쪽 의도)
- [ ] 사용자 선택 후 해결 적용
- [ ] 머지 완료

---

### 4.3 Dry-run 모드

**테스트 목적**: `--dry-run`으로 실제 머지 없이 확인만 가능한지

```bash
/team-claude:merge --dry-run
```

**체크포인트**:
- [ ] 머지 계획만 표시
- [ ] 실제 Git 명령어 실행 안 됨
- [ ] Conflict 예상 여부 표시

---

## Phase 5: 통합 E2E 테스트

### 5.1 전체 워크플로우 테스트

**테스트 목적**: 처음부터 끝까지 전체 흐름이 정상 동작하는지 확인

**시나리오**:
```bash
# 1. 서버 시작
/team-claude:setup server

# 2. 설계
/team-claude:architect "Calculator에 power(거듭제곱) 기능 추가"
# → 대화형 설계 진행
# → Contract + Checkpoint 생성
# → 승인

# 3. 구현 위임
/team-claude:delegate --all
# → Worker 실행
# → 자동 검증
# → PR 생성

# 4. 머지
/team-claude:merge
# → 자동/Semi-Auto 머지
# → main PR 생성

# 5. 완료 확인
git log --oneline
npm test  # 모든 테스트 통과
```

**체크포인트**:
- [ ] 전체 흐름 완료
- [ ] 최종 코드가 올바름
- [ ] 모든 테스트 통과
- [ ] Git 히스토리 정상

---

### 5.2 Context 관리 테스트

**테스트 목적**: Context 80% 도달 시 checkpoint + /clear가 동작하는지

**시나리오**:
- 큰 태스크를 위임
- Worker가 오래 작업하여 Context 80% 도달
- Statusline hook이 경고
- Worker가 checkpoint 저장 후 /clear

**체크포인트**:
- [ ] Context 경고 메시지 발생
- [ ] `.team-claude-checkpoint.md` 생성됨
- [ ] `/clear` 후 작업 재개
- [ ] 최종 완료

---

## Edge Cases & 에러 처리

### E1. 서버 연결 실패

**시나리오**: 서버가 실행 중이지 않을 때 `/team-claude:delegate` 실행

**예상 동작**:
```
⚠️ Team Claude 서버가 실행 중이지 않습니다.

/team-claude:setup server 를 먼저 실행하세요.
```

**체크포인트**:
- [ ] 에러 메시지 명확
- [ ] 복구 방법 안내

---

### E2. 세션 없음

**시나리오**: 존재하지 않는 세션 ID로 delegate 시도

```bash
/team-claude:delegate --session invalid123
```

**예상 동작**:
```
❌ 세션을 찾을 수 없습니다: invalid123

현재 세션 목록:
  - abc12345: Calculator 기능 추가 (설계 완료)
```

**체크포인트**:
- [ ] 에러 메시지 명확
- [ ] 사용 가능한 세션 목록 표시

---

### E3. Checkpoint 미승인

**시나리오**: 승인되지 않은 Checkpoint에 delegate 시도

**예상 동작**:
```
⏸️ Checkpoint가 아직 승인되지 않았습니다.

/team-claude:architect --resume abc12345 로 설계를 완료하세요.
```

**체크포인트**:
- [ ] 미승인 상태 감지
- [ ] 복구 방법 안내

---

### E4. Worker 비정상 종료

**시나리오**: Worker Claude가 비정상 종료 (예: 사용자가 탭 닫음)

**예상 동작**:
- 서버가 timeout 후 실패 처리
- 재시도 또는 에스컬레이션

**체크포인트**:
- [ ] Timeout 감지
- [ ] 적절한 에러 처리
- [ ] 재시도 가능

---

### E5. Git Worktree 충돌

**시나리오**: 이미 존재하는 브랜치로 Worktree 생성 시도

**예상 동작**:
- 기존 Worktree 사용 또는 에러

**체크포인트**:
- [ ] 충돌 감지
- [ ] 적절한 복구

---

### E6. 네트워크 타임아웃

**시나리오**: 서버 API 호출 중 타임아웃

**예상 동작**:
- 재시도 로직
- 최대 재시도 후 에러

**체크포인트**:
- [ ] 타임아웃 감지
- [ ] 재시도 수행
- [ ] 최종 에러 메시지

---

## 테스트 실행 체크리스트

### Phase 1: 서버
- [ ] 1.1 서버 빌드 및 시작
- [ ] 1.2 서버 API 테스트
- [ ] 1.3 Executor 테스트

### Phase 2: Architect
- [ ] 2.1 새 설계 세션 시작
- [ ] 2.2 설계 대화 진행
- [ ] 2.3 Contract 생성
- [ ] 2.4 Checkpoint 승인

### Phase 3: Delegate
- [ ] 3.1 단일 Checkpoint 위임
- [ ] 3.2 자동 검증 루프
- [ ] 3.3 피드백 루프 (재시도)
- [ ] 3.4 병렬 위임
- [ ] 3.5 에스컬레이션

### Phase 4: Merge
- [ ] 4.1 자동 머지
- [ ] 4.2 Conflict 해결
- [ ] 4.3 Dry-run 모드

### Phase 5: E2E
- [ ] 5.1 전체 워크플로우
- [ ] 5.2 Context 관리

### Edge Cases
- [ ] E1. 서버 연결 실패
- [ ] E2. 세션 없음
- [ ] E3. Checkpoint 미승인
- [ ] E4. Worker 비정상 종료
- [ ] E5. Git Worktree 충돌
- [ ] E6. 네트워크 타임아웃

---

## 다음 단계

테스트 시나리오 실행 후:

1. **버그 발견 시**: GitHub Issue 생성
2. **개선점 발견 시**: 문서 또는 코드 수정
3. **테스트 통과 시**: 체크리스트에 체크 표시

모든 테스트 통과 후 v0.2.0 릴리스 진행.
