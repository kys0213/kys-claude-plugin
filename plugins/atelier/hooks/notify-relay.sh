#!/usr/bin/env bash
# notify-relay.sh <ask-question|notification> — hook 이벤트를 메시지 채널로 중계
#   PreToolUse(AskUserQuestion) → ask-question : 질문 내용 전달
#   Notification               → notification : 권한 요청·유휴 대기 알림 전달
#
# 책임 경계 (tool-layer-boundary): 이 shim은 부트스트랩(바이너리 존재 확인)만 담당하고,
# 페이로드 파싱·채널 결정·전송은 모두 `atelier notify <sub>`(CLI)가 수행합니다.
#
# advisory(비차단) hook — 어떤 경우에도 exit 0:
#   - atelier CLI 미설치 → 무음 no-op (미사용 환경 배려)
#   - 채널 미설정 / 전송 실패 → CLI가 리포트만 출력하고 exit 0

command -v atelier &> /dev/null || { cat > /dev/null; exit 0; }

atelier notify "${1:-ask-question}" || true
exit 0
