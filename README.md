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

## 요구사항

- Claude Code CLI
- (선택) OpenAI Codex CLI - `/review-codex`, `/invoke-codex` 사용 시
- (선택) Google Gemini CLI - `/review-gemini`, `/invoke-gemini` 사용 시

## 작성자

kys0213
