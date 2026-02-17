# KYS Claude Plugin

Claude Code 플러그인 모음

## 빠른 설치

```bash
# 마켓플레이스 추가
/plugin marketplace add kys0213/kys-claude-plugin

# develop-workflow 플러그인 설치 (설계/리뷰/구현 통합 워크플로우)
/plugin install develop-workflow@kys-claude-plugin

# external-llm 플러그인 설치 (외부 LLM 호출)
/plugin install external-llm@kys-claude-plugin
```

## Runtime Requirements

This project requires [Bun](https://bun.sh) runtime (v1.0+) for the following components:
- `plugins/team-claude/cli` - Team Claude CLI tool
- `plugins/team-claude/server` - Team Claude MCP server

### Installation

```bash
curl -fsSL https://bun.sh/install | bash
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
    ├── develop-workflow/   # 통합 개발 워크플로우 (설계/리뷰/구현)
    ├── external-llm/      # 외부 LLM 호출 인프라
    ├── git-utils/         # Git 워크플로우 자동화
    ├── suggest-workflow/  # 세션 분석 기반 워크플로우 제안
    ├── team-claude/       # 멀티 에이전트 협업 시스템
    └── workflow-guide/    # 에이전트 설계 원칙 가이드
```

## 플러그인

### develop-workflow

통합 개발 워크플로우: 설계 → 리뷰 → 구현을 하나의 파이프라인으로 (멀티 LLM 지원)

**Commands:**
- `/outline` - 상위 레벨 아키텍처 설계 (멀티 LLM)
- `/design` - Contract 기반 상세 설계 (멀티 LLM)
- `/develop` - 전체 워크플로우 (설계 → 리뷰 → 구현)
- `/implement` - 구현 단계
- `/multi-review` - 멀티 LLM 코드 리뷰

**사용:**
```bash
claude --plugin-dir /path/to/plugins/develop-workflow
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
# 의존성 설치
npm install

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
- Node.js 20+ (개발 시)
- (선택) OpenAI Codex CLI - `/invoke-codex` 사용 시
- (선택) Google Gemini CLI - `/invoke-gemini` 사용 시

## 작성자

kys0213
