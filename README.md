# KYS Claude Plugin

Claude Code 플러그인 모음

## 구조

```
kys-claude-plugin/
├── common/
│   └── scripts/           # 공유 스크립트
│       ├── call-codex.sh
│       └── call-gemini.sh
│
└── plugins/
    ├── review/            # 다중 LLM 리뷰 시스템
    └── external-llm/      # 외부 LLM 호출 인프라
```

## 플러그인

### review

다양한 LLM(Claude, Codex, Gemini)을 사용한 문서 리뷰 시스템

**Commands:**
- `/review-claude` - Claude로 문서 리뷰
- `/review-codex` - OpenAI Codex로 문서 리뷰
- `/review-gemini` - Google Gemini로 문서 리뷰
- `/review-all` - 3개 LLM 종합 리뷰

**사용:**
```bash
claude --plugin-dir /path/to/plugins/review
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

## 마켓플레이스로 설치

```bash
# 마켓플레이스 추가
/plugin marketplace add kys0213/kys-claude-plugin

# 플러그인 설치
/plugin install review@kys-claude-plugin
/plugin install external-llm@kys-claude-plugin
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
- **PR 타이틀 규칙**: Conventional Commits 형식
  - `feat:` → MINOR 버전 bump (0.1.0 → 0.2.0)
  - `fix:` → PATCH 버전 bump (0.1.0 → 0.1.1)
  - `major:` → MAJOR 버전 bump (0.1.0 → 1.0.0)
- **Merge 시**: 자동 버전 bump 및 릴리스 태그 생성

## 요구사항

- Claude Code CLI
- Node.js 20+ (개발 시)
- (선택) OpenAI Codex CLI - `/review-codex`, `/invoke-codex` 사용 시
- (선택) Google Gemini CLI - `/review-gemini`, `/invoke-gemini` 사용 시

## 작성자

kys0213
