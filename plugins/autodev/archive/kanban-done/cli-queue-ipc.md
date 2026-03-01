# CLI queue 서브커맨드 + IPC 설계 (M-3)

> **Priority**: Medium — 운영 시 queue 상태 확인/재시도 불가
> **분석 리포트**: design-implementation-analysis.md §3-2
> **난이도**: 높음

## 배경

README.md에 `autodev queue list/retry/clear` 서브커맨드가 문서화되어 있으나,
InMemory queue 전환 후 CLI에서 daemon의 queue에 접근할 방법이 없음.
REFACTORING-PLAN.md에서도 "client/mod.rs는 이번 scope 제외 — IPC 설계 필요"로 명시.

현재는 `daemon.status.json` 읽기를 통해 조회만 가능하고 retry/clear 같은 쓰기 작업 불가.

## 선택지

### Option A: Unix Domain Socket IPC

- daemon이 UDS 서버 리슨
- CLI가 소켓 연결 후 JSON-RPC 형태로 요청
- 장점: 실시간, 양방향
- 단점: 구현 복잡도 높음

### Option B: 파일 기반 커맨드 큐

- CLI가 `~/.autodev/commands/` 에 커맨드 파일 작성
- daemon이 매 tick에서 파일 감시 후 처리
- 장점: 구현 간단
- 단점: 지연 있음 (tick 주기만큼)

### Option C: 현실적 축소 — 조회만 지원 + 문서 갱신

- `autodev queue list` → `daemon.status.json` 기반 조회만 구현
- retry/clear는 daemon 재시작으로 대체
- README.md에서 retry/clear 제거
- 장점: 최소 공수
- 단점: 운영 편의성 제한

## 항목

- [ ] **1. IPC 방식 결정**
  - Option A/B/C 중 선택
  - 설계 문서 작성

- [ ] **2. 구현** (선택 방식에 따라)
  - `autodev queue list <repo>` — 최소한 이것은 구현
  - retry/clear는 선택적

- [ ] **3. README.md 갱신**
  - 실제 구현된 커맨드만 문서화

## 완료 조건

- [ ] `autodev queue list`로 현재 큐 상태 조회 가능
- [ ] README.md가 실제 CLI와 일치
