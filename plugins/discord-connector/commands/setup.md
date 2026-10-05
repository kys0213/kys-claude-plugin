---
description: discord CLI 의 on_message 훅이 부를 런처 shim 을 설치하고 CLI 설정 예시를 안내합니다
argument-hint: ""
allowed-tools: ["Bash", "Read", "Write"]
---

# discord-connector setup

`discord` CLI 데몬이 멘션을 감지하면 실행할 **런처 shim**(`~/.local/bin/discord-connector-launch`)을 설치합니다.
shim 은 설치된 플러그인 중 가장 높은 버전의 `bin/launch.mjs` 를 찾아 실행해요. 그래서 플러그인을 업데이트해도 CLI 설정의 `on_message` 경로를 바꿀 필요가 없어요.

이 커맨드는 `~/.claude`·`~/.areum` 설정을 건드리지 않고, `discord` 데몬도 띄우지 않습니다. 설치하는 것은 shim 파일 하나뿐입니다.

## Step 1 — 사전 조건 확인

```bash
node --version
command -v discord
test -f ~/.areum/discord/config.json && echo "config 있음" || echo "config 없음"
```

- `node` 가 22 이상이 아니면 중단하고 안내합니다.
- `discord` 가 PATH 에 없으면 중단하고 설치를 안내합니다.
- config 파일이 없으면 중단하고 `echo "$TOKEN" | discord init` 을 안내합니다. 토큰이 설정 파일에 있어야 해요. 훅 실행 환경에는 `DISCORD_BOT_TOKEN` 이 넘어오지 않기 때문입니다.
- config 파일 내용은 출력하지 않습니다 (토큰이 평문이에요).

## Step 2 — shim 설치

기존 파일이 있으면 덮어쓸지 사용자에게 먼저 묻습니다. 없거나 동의를 받았으면 아래 heredoc 을 그대로 실행합니다.

```bash
mkdir -p ~/.local/bin
cat > ~/.local/bin/discord-connector-launch <<'SHIM'
#!/bin/sh
set -eu
base="${CLAUDE_CODE_PLUGIN_CACHE_DIR:-$HOME/.claude/plugins}/cache"
launcher=$(ls -d "$base"/*/discord-connector/*/bin/launch.mjs 2>/dev/null | sort -V | tail -n 1)
if [ -z "$launcher" ]; then
  echo "discord-connector 플러그인이 설치돼 있지 않아요" >&2
  exit 1
fi
exec node "$launcher"
SHIM
chmod +x ~/.local/bin/discord-connector-launch
```

설치 후 확인합니다.

```bash
test -x ~/.local/bin/discord-connector-launch && echo "설치됨"
```

## Step 3 — CLI 설정 안내

`~/.areum/discord/config.json` 을 사용자가 직접 편집하도록 아래 예시를 보여 줍니다. 이 커맨드는 파일을 수정하지 않아요.

```json
{
  "on_message": ["~/.local/bin/discord-connector-launch"],
  "default_workdir": "~/workspace/my-project",
  "workdirs": {
    "<채널 ID 또는 channels 별칭>": "~/workspace/other-project"
  }
}
```

- `on_message`: argv 배열이에요. 위 shim 경로를 그대로 쓰세요.
- `default_workdir`: 매핑이 없는 채널에서 Claude 가 일할 디렉토리예요. `on_message` 를 쓰면 `workdirs` 나 `default_workdir` 중 하나는 꼭 있어야 해요.
- `workdirs`: 채널별 디렉토리 매핑이에요. 경로는 절대경로이거나 `~/` 로 시작해야 해요.
- 이슈 채널 목록과 `trigger_bots` 도 같은 파일에 둡니다. 정확한 키 이름은 `discord` CLI 문서를 따르세요.
- 설정을 바꾼 뒤에는 데몬을 재시작해야 반영돼요.

## 에러 처리

| 상황 | 동작 |
|------|------|
| node 22 미만 / discord 없음 / config 없음 | shim 을 설치하지 않고 이유와 해결 방법을 안내 |
| 쓰기 실패 | 오류를 그대로 보여 주고 중단 |

## Output Examples

성공:

```
shim 설치: ~/.local/bin/discord-connector-launch
다음 단계: ~/.areum/discord/config.json 에 on_message 를 추가하고 데몬을 재시작하세요.
```

실패:

```
중단: discord CLI 를 찾지 못했어요. 설치 후 다시 실행해 주세요.
```
