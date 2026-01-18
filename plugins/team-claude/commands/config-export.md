---
name: config-export
description: Team Claude 설정 내보내기 - JSON/YAML 파일 또는 공유 URL 생성
argument-hint: "[--format] [--output]"
allowed-tools: ["Bash", "Read", "Write", "AskUserQuestion"]
---

# Team Claude Config Export

현재 설정을 파일 또는 공유 가능한 형태로 내보냅니다.

## 사용법

```bash
/team-claude:config-export [options]
```

## 옵션

| 옵션 | 설명 | 기본값 |
|------|------|--------|
| `--format` | 출력 형식 (json/yaml) | json |
| `--output` | 출력 파일 경로 | stdout |
| `--templates` | 사용자 템플릿 포함 | false |
| `--rules` | 리뷰 규칙 포함 | false |
| `--sensitive` | 민감 정보 포함 | false |

## API 연동

```bash
# 기본 내보내기
curl -s "http://localhost:3847/config/export" | jq

# 템플릿 + 규칙 포함
curl -s "http://localhost:3847/config/export?templates=true&rules=true" | jq

# 민감 정보 포함
curl -s "http://localhost:3847/config/export?sensitive=true" | jq
```

## 대화형 내보내기

```
> /team-claude:config-export

📤 설정 내보내기

포함할 항목을 선택하세요 (여러 개 선택 가능):
  [x] 기본 설정 (server, worker, notification, review)
  [ ] 사용자 템플릿
  [ ] 리뷰 규칙
  [ ] 민감 정보 (webhook URL 등)

출력 형식:
  1. json - 전체 설정 파일
  2. yaml - 가독성 좋은 형식
  3. clipboard - 클립보드에 복사
  4. file - 파일로 저장
선택 [1]: 1

╔══════════════════════════════════════════════════════════════╗
║               Exported Configuration                         ║
╠══════════════════════════════════════════════════════════════╣
{
  "version": "1.0",
  "server": {
    "port": 3847,
    "host": "localhost"
  },
  "worker": {
    "maxConcurrent": 5,
    "defaultTemplate": "standard"
  },
  ...
}
╚══════════════════════════════════════════════════════════════╝

✅ 설정 내보내기 완료

팀원에게 공유: /team-claude:config-import '<JSON>'
```

## 파일로 저장

```bash
# JSON 파일로 저장
/team-claude:config-export --output team-claude-config.json

# YAML 파일로 저장
/team-claude:config-export --format yaml --output team-claude-config.yaml
```

## 민감 정보 처리

기본적으로 민감 정보는 마스킹됩니다:

```json
{
  "notification": {
    "method": "slack",
    "slack": {
      "webhookUrl": "***",  // 마스킹됨
      "channel": "#team-claude"
    }
  }
}
```

`--sensitive` 옵션으로 실제 값 포함:

```bash
/team-claude:config-export --sensitive
```

## 출력 형식 예시

### JSON
```json
{
  "version": "1.0",
  "server": {
    "port": 3847,
    "host": "localhost",
    "timeout": 60000
  },
  "worker": {
    "maxConcurrent": 5,
    "defaultTemplate": "standard",
    "timeout": 1800
  }
}
```

### YAML
```yaml
version: "1.0"
server:
  port: 3847
  host: localhost
  timeout: 60000
worker:
  maxConcurrent: 5
  defaultTemplate: standard
  timeout: 1800
```

## 부분 내보내기

특정 섹션만 내보내기:

```bash
# worker 설정만
curl -s "http://localhost:3847/config/worker" | jq

# notification 설정만
curl -s "http://localhost:3847/config/notification" | jq
```

## 팀 공유 워크플로우

```
1. 설정 담당자가 내보내기
   /team-claude:config-export --templates --rules --output shared-config.json

2. 설정 파일을 팀 저장소에 커밋
   git add shared-config.json
   git commit -m "chore: update team-claude config"

3. 팀원이 가져오기
   /team-claude:config-import shared-config.json
```

## 관련 커맨드

- `/team-claude:config-import` - 설정 가져오기
- `/team-claude:config` - 설정 조회/수정
- `/team-claude:setup` - 초기 설정 위자드
