---
description: 자율 개발 데몬 제어 - 시작, 중지, 상태 확인
argument-hint: "[start|stop|status]"
allowed-tools: ["Bash"]
---

# 자율 개발 데몬 제어 (/auto)

autonomous 데몬의 시작, 중지, 상태를 제어합니다.

## 사용법

- `/auto start` - 데몬 시작 (백그라운드)
- `/auto stop` - 데몬 중지
- `/auto status` - 상태 요약 (등록된 레포, 큐 현황)
- `/auto` (인자 없음) - 현재 상태 요약 출력

## 실행

### Step 0: CLI 버전 확인

명령 실행 전 `ensure-binary.sh`로 바이너리가 최신인지 확인합니다:

```bash
bash ${CLAUDE_PLUGIN_ROOT}/scripts/ensure-binary.sh
```

실패 시 바이너리 설치/업데이트를 안내하고 중단합니다.

### Step 1: 명령 실행

인자에 따라 적절한 `autonomous` CLI 명령을 실행하세요:

1. 인자가 `start`인 경우:
   ```bash
   autonomous start
   ```

2. 인자가 `stop`인 경우:
   ```bash
   autonomous stop
   ```

3. 인자가 `status`이거나 없는 경우:
   ```bash
   autonomous status
   ```

실행 결과를 사용자에게 보기 좋게 정리하여 출력하세요.
