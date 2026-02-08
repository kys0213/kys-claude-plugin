---
description: 세션 분석 기반 워크플로우 제안 - Claude Code 활용 최적화
---

# Suggest Workflow

프로젝트의 세션 히스토리를 분석하여 반복적인 워크플로우를 감지하고 자동화 제안을 생성합니다.

## 사용법

Rust CLI를 사용합니다:

```bash
${CLAUDE_PLUGIN_ROOT}/cli/target/release/suggest-workflow workflow \
  --source projects \
  --project "$(pwd)" \
  --threshold 5 \
  --top 10 \
  --format text
```

## 분석 항목

- **프롬프트 빈도**: 가장 자주 사용되는 프롬프트 패턴
- **도구 사용 패턴**: 자주 사용되는 도구 조합
- **워크플로우 시퀀스**: 반복되는 작업 순서
- **행동 클러스터**: 유사한 작업 패턴 그룹화

## 옵션

- `--source <history|projects>`: 데이터 소스 (기본: projects) - `history` source is (planned - not yet implemented)
- `--threshold N`: 최소 반복 횟수 (기본: 5)
- `--top N`: 상위 N개 결과만 표시 (기본: 10)
- `--project PATH`: 프로젝트 경로 (기본: 현재 디렉토리)
- `--report`: 마크다운 리포트 생성 (planned - not yet implemented)
- `--format <text|json>`: 출력 형식 (기본: text)
- `--decay`: 시간 감쇠 가중치 활성화
- `--gap-tolerant`: 갭 허용 시퀀스 매칭 (planned - not yet implemented)

## 출력 예시

분석 결과는 다음과 같은 형태로 제공됩니다:

1. **워크플로우 요약**: 전체 세션 수, 분석 기간, 주요 패턴
2. **반복 패턴**: 빈도가 높은 프롬프트와 도구 사용
3. **제안 사항**: 자동화 가능한 워크플로우 스킬 제안

## 설치

먼저 CLI를 빌드해야 합니다:

```bash
/suggest-workflow:setup
```
