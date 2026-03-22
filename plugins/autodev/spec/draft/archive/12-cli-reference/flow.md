# Flow 12: 인터페이스 레퍼런스

### 아키텍처 (3-layer)

```
Layer 1: Slash Command (3개, thin wrapper)
  /auto, /spec, /claw

Layer 2: DataSource + AgentRuntime (OCP 확장점)
  → 외부 시스템 lifecycle + LLM 실행 추상화

Layer 3: autodev CLI (SSOT)
  → DB 조작, 상태 전이, 코어 로직
  → 모든 레이어가 CLI를 호출
```

### SSOT 보장

```
/spec add       → autodev spec add (CLI)
/claw 세션      → Bash: autodev queue list --json (CLI)
DataSource hook → db.queue_advance (코어, CLI 내부)
Cron script     → autodev agent (CLI)
```

### Plugin 구조

```
plugins/autodev/
├── .claude-plugin/plugin.json     # 3 commands, 3 agents
├── commands/
│   ├── auto.md                    # /auto
│   ├── spec.md                    # /spec
│   └── claw.md                    # /claw
├── agents/
│   ├── issue-analyzer.md
│   ├── pr-reviewer.md
│   └── conflict-resolver.md
└── skills/
    ├── cli-reference/
    └── label-setup/
```

### Slash Command 매핑

| v4 | v5 |
|----|-----|
| /auto, /auto-setup, /auto-config, /auto-dashboard, /update | /auto (서브커맨드) |
| /add-spec, /update-spec, /spec | /spec (서브커맨드) |
| /status, /board, /decisions, /hitl, /repo, /claw, /cron | /claw (자연어) |

### autodev CLI 전체 참조

```
autodev
├── start / stop / restart
├── status [--format text|json|rich]
├── dashboard
├── repo
│   ├── add / list / show / update / remove / config
├── spec
│   ├── add / list / show / update
│   ├── pause / resume / complete / remove
│   ├── link / unlink
│   ├── status <id> / verify <id> / decisions <id>
├── queue
│   ├── list / show
│   ├── advance / skip
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
├── agent [--repo <name>] [-p <prompt>]
├── convention
├── worktree list / clean
├── board [--format text|json|rich]
├── logs / usage / report
```

모든 서브커맨드는 `--json` 또는 `--format json` 출력 지원.

---

### 관련 플로우

- [Flow 10: Claw 워크스페이스](../10-claw-workspace/flow.md)
- [Flow 14: 시각화](../14-visualization/flow.md)
