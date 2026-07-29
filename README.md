# KYS Claude Plugin

Claude Code 플러그인 모음

## 빠른 설치

```bash
# 마켓플레이스 추가
/plugin marketplace add kys0213/kys-claude-plugin

# atelier 플러그인 설치 (spec 설계 → 리뷰 → 구현 → git/PR 워크플로우 통합)
/plugin install atelier@kys-claude-plugin

# external-llm 플러그인 설치 (외부 LLM 호출)
/plugin install external-llm@kys-claude-plugin
```

## 구조

```
kys-claude-plugin/
├── common/
│   └── scripts/           # 공유 스크립트
│       ├── call-codex.sh
│       └── call-gemini.sh
│
└── plugins/
    ├── atelier/           # 통합 개발 워크플로우 (spec 설계 → 리뷰 → 구현 → git/PR)
    ├── external-llm/      # 외부 LLM 호출 인프라
    ├── hud/               # Claude Code 상태줄 (색상·진행률·클릭 링크)
    ├── suggest-workflow/  # 세션 분석 기반 워크플로우 제안
    ├── barrier-sync/      # 병렬 백그라운드 Task 동기화 (FIFO barrier)
    └── openclaw-docker/   # OpenClaw Docker 환경 관리
```

## 플러그인

### atelier

통합 개발 워크플로우: spec 설계 → 리뷰 → 구현 → PR 머지까지 하나의 책임 경계 안에서 제공 (자세한 내용은 `plugins/atelier/README.md` 참고)

**Skills (슬래시 + 모델 자동 호출):**
- `/atelier:spec-write` - 합의된 설계를 스펙 문서 계층(DESIGN→concerns→flows)으로 작성
- `/atelier:communicate` - 맥락을 팀에 전달하는 작문 기준 (독자 수준·맥락 이전·채널 적응)
- `/atelier:git` - git 워크플로우 (커밋·push·PR·충돌 해결·리뷰 정리·이슈 우선순위)
- `/atelier:workflow` - 컨벤션 scaffold·.claude/rules 설계·설계 원칙 룰 설치
- `/atelier:orchestrator` - 위임/병렬 분해·worktree 격리·머지 조정 (기본 자율 주행)
- `/atelier:grill` - 설계를 대화로 생성하거나 이미 있는 계획을 심문

**Command:**
- `/atelier:setup` - 통합 setup (git / style / workflow 모듈 + hook 관리)

**사용:**
```bash
claude --plugin-dir /path/to/plugins/atelier
```

### external-llm

외부 LLM(OpenAI Codex, Google Gemini) 호출 인프라

**Commands:**
- `/invoke-codex` - Codex CLI 범용 호출
- `/invoke-gemini` - Gemini CLI 범용 호출

**사용:**
```bash
claude --plugin-dir /path/to/plugins/external-llm
```

## 개발

### 검증 도구

```bash
# 전체 검증 실행
npm run validate

# 개별 검증
npm run validate:specs     # 스펙 검증
npm run validate:paths     # 경로 검증 (AST 기반)
npm run validate:versions  # 버전 검증
```

### CI/CD

- **PR 생성 시**: 자동으로 스펙, 경로, 버전 검증
- **PR 타이틀 규칙**: Conventional Commits 형식 (상세: `.claude/rules/git-workflow.md`)
- **Merge 시**: 자동 버전 bump 및 릴리스 태그 생성

## Claude Code 문서

| 기능 | 링크 |
|------|------|
| Skills | https://code.claude.com/docs/en/skills |
| Subagents | https://code.claude.com/docs/en/sub-agents |
| Slash Commands | https://code.claude.com/docs/en/slash-commands |
| Hooks | https://code.claude.com/docs/en/hooks-guide |
| Plugins | https://code.claude.com/docs/en/plugins |
| Plugins Reference | https://code.claude.com/docs/en/plugins-reference |
| Plugin Marketplaces | https://code.claude.com/docs/en/plugin-marketplaces |
| Discover Plugins | https://code.claude.com/docs/en/discover-plugins |

### Marketplace 참고 자료

| 자료 | 링크 |
|------|------|
| 공식 Marketplace 예시 | https://github.com/anthropics/claude-code/blob/main/.claude-plugin/marketplace.json |
| 공식 플러그인 모음 | https://github.com/anthropics/claude-plugins-official |
| Marketplace Schema | https://anthropic.com/claude-code/marketplace.schema.json |

### marketplace.json `strict` 필드

| 값 | 의미 |
|----|------|
| `strict: true` (기본값) | 플러그인에 자체 `plugin.json` 필요, marketplace 필드는 보조 |
| `strict: false` | `plugin.json` 불필요, marketplace 엔트리가 전체 매니페스트 역할 |

## 요구사항

- Claude Code CLI
- Go 1.21+ (`tools/` 빌드 시)
- Rust 1.93.1 (`plugins/atelier/cli`, `plugins/suggest-workflow/cli` 빌드 시)
- (선택) Node.js (npm → make 래퍼 스크립트 사용 시)
- (선택) OpenAI Codex CLI - `/invoke-codex` 사용 시
- (선택) Google Gemini CLI - `/invoke-gemini` 사용 시

## 작성자

kys0213
