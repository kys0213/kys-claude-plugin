# CLI 레퍼런스 — 3-layer 아키텍처 + 전체 커맨드 트리

> autodev CLI는 모든 레이어의 SSOT(Single Source of Truth)이다.

---

## 아키텍처 (3-layer)

```
Layer 1: Slash Command (3개, thin wrapper)
  /auto, /spec, /claw

Layer 2: DataSource + AgentRuntime (OCP 확장점)
  → 외부 시스템 워크플로우 + LLM 실행 추상화

Layer 3: autodev CLI (SSOT)
  → DB 조작, 상태 전이, 코어 로직
  → 모든 레이어가 CLI를 호출
```

---

## Slash Command 매핑 (v4 → v5)

| v4 | v5 |
|----|-----|
| /auto, /auto-setup, /auto-config, /auto-dashboard, /update | /auto (서브커맨드) |
| /add-spec, /update-spec, /spec | /spec (서브커맨드) |
| /status, /board, /decisions, /hitl, /repo, /claw, /cron | /claw (자연어) |

---

## autodev CLI 전체 참조

```
autodev
├── start / stop / restart
├── status [--format text|json|rich]
├── dashboard
├── workspace
│   ├── add / list / show / update / remove / config
├── spec
│   ├── add / list / show / update
│   ├── pause / resume / complete / remove
│   ├── link / unlink
│   ├── status <id> / verify <id> / decisions <id>
├── queue
│   ├── list / show / skip
│   └── dependency add / remove
├── hitl
│   ├── list / show / respond / timeout
├── cron
│   ├── list / add / update
│   ├── pause / resume / remove / trigger
├── claw
│   ├── init / rules / edit
├── decisions
│   ├── list / show
├── agent [--workspace <name>] [-p <prompt>]
├── convention
├── worktree list / clean
├── board [--format text|json|rich]
├── logs / usage / report
```

모든 서브커맨드는 `--json` 또는 `--format json` 출력 지원.
