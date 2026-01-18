---
name: config-import
description: Team Claude 설정 가져오기 - JSON/YAML 파일 또는 URL에서 설정 로드
argument-hint: "<source>"
allowed-tools: ["Bash", "Read", "Write", "AskUserQuestion", "WebFetch"]
---

# Team Claude Config Import

외부 소스에서 설정을 가져와 적용합니다.

## 사용법

```bash
/team-claude:config-import <source> [--scope]
```

## 소스 유형

| 소스 | 예시 |
|------|------|
| 파일 경로 | `./team-claude-config.json` |
| JSON 문자열 | `'{"worker": {"maxConcurrent": 10}}'` |
| URL | `https://gist.githubusercontent.com/.../config.json` |

## 옵션

| 옵션 | 설명 | 기본값 |
|------|------|--------|
| `--scope` | 저장 스코프 (global/project) | project |
| `--merge` | 기존 설정과 병합 | true |
| `--force` | 확인 없이 적용 | false |

## API 연동

```bash
curl -X POST http://localhost:3847/config/import \
  -H "Content-Type: application/json" \
  -d '{
    "config": {
      "worker": {"maxConcurrent": 10},
      "review": {"autoLevel": "full-auto"}
    },
    "scope": "project"
  }'
```

## 대화형 가져오기

```
> /team-claude:config-import ./shared-config.json

📥 설정 가져오기

소스: ./shared-config.json
버전: 1.0

╔══════════════════════════════════════════════════════════════╗
║               Configuration Changes                          ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  변경되는 항목:                                               ║
║                                                              ║
║  worker.maxConcurrent                                        ║
║    현재: 5 → 새 값: 10                                        ║
║                                                              ║
║  review.autoLevel                                            ║
║    현재: semi-auto → 새 값: full-auto                         ║
║                                                              ║
║  추가되는 템플릿:                                             ║
║    + custom-frontend                                         ║
║                                                              ║
║  추가되는 규칙:                                               ║
║    + no-magic-numbers                                        ║
║                                                              ║
╚══════════════════════════════════════════════════════════════╝

⚠️  review.autoLevel이 full-auto로 변경됩니다.
    Worker가 무한 루프에 빠질 수 있으니 주의하세요.

적용하시겠습니까? [y/N]: y

✅ 설정 가져오기 완료
   변경된 항목: 4개
```

## 파일에서 가져오기

```bash
# JSON 파일
/team-claude:config-import ./team-claude-config.json

# YAML 파일
/team-claude:config-import ./team-claude-config.yaml
```

## JSON 문자열로 가져오기

```bash
# 간단한 설정 변경
/team-claude:config-import '{"worker": {"maxConcurrent": 10}}'

# 여러 설정 변경
/team-claude:config-import '{
  "worker": {"maxConcurrent": 10, "timeout": 3600},
  "review": {"autoLevel": "full-auto"}
}'
```

## URL에서 가져오기

```bash
# Gist에서 가져오기
/team-claude:config-import https://gist.githubusercontent.com/user/abc123/raw/config.json

# 팀 저장소에서 가져오기
/team-claude:config-import https://raw.githubusercontent.com/team/configs/main/team-claude.json
```

## 스코프 지정

```bash
# 프로젝트 설정으로 저장 (기본)
/team-claude:config-import ./config.json --scope project

# 글로벌 설정으로 저장
/team-claude:config-import ./config.json --scope global
```

## 병합 vs 덮어쓰기

기본적으로 기존 설정과 병합됩니다:

```bash
# 병합 (기본) - 지정된 값만 변경, 나머지 유지
/team-claude:config-import '{"worker": {"maxConcurrent": 10}}'

# 섹션 전체 교체
/team-claude:config-import '{"worker": {"maxConcurrent": 10}}' --no-merge
```

## 검증

가져오기 전 자동 검증:

```
> /team-claude:config-import '{"server": {"port": -1}}'

❌ 가져오기 실패

유효하지 않은 설정:
  - server.port: -1은 1024-65535 사이여야 합니다.
```

## 부분 가져오기

특정 섹션만 가져오기:

```bash
# worker 설정만 가져오기
/team-claude:config-import '{"worker": {"maxConcurrent": 10}}'

# 템플릿만 가져오기
/team-claude:config-import '{"templates": {"custom": {...}}}'

# 규칙만 가져오기
/team-claude:config-import '{"review": {"rules": [...]}}'
```

## 백업 및 롤백

가져오기 전 자동 백업:

```
백업 저장: .team-claude/config.backup.json

롤백하려면:
  /team-claude:config-import .team-claude/config.backup.json
```

## 팀 공유 예시

```bash
# 팀 리더가 설정 내보내기
/team-claude:config-export --templates --rules > team-config.json

# Git으로 공유
git add team-config.json
git commit -m "chore: share team-claude config"
git push

# 팀원이 가져오기
git pull
/team-claude:config-import team-config.json
```

## 관련 커맨드

- `/team-claude:config-export` - 설정 내보내기
- `/team-claude:config` - 설정 조회/수정
- `/team-claude:setup` - 초기 설정 위자드
