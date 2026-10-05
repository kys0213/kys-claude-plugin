---
paths:
  - "plugins/*/.claude-plugin/plugin.json"
  - ".claude-plugin/marketplace.json"
  - "plugins/*/skills/**"
  - "plugins/*/commands/**"
  - "plugins/*/agents/**"
---

# 플러그인 간 의존 선언

플러그인이 다른 플러그인의 skill·command·agent 를 호출하면, 그 의존을 `plugin.json` 에 드러낸다. 선언이 없는 교차 호출은 묵시적 의존이다. 대상 플러그인이 없는 환경에서 조용히 깨진다.

의존 메커니즘은 공식 `plugin.json` 의 `dependencies` 하나만 쓴다. 자체 필드·컨벤션을 만들지 않는다. 아무도 해석하지 않아 조용히 무시된다.

## 규칙

- DO: 다른 플러그인의 skill·command·agent 를 호출하면 호출하는 쪽 `plugin.json` 에 `"dependencies": ["<plugin>"]` 을 선언한다.
- DO: bare name 만 쓴다. 같은 마켓플레이스 안의 이름으로 해석된다.
- DON'T: 버전 범위를 쓰지 않는다. 범위는 `<plugin>--v<version>` git tag 가 있어야 풀리는데, 이 레포는 그 tag 를 만들지 않는다. 같은 레포의 상대경로 플러그인은 마켓플레이스의 현재 사본을 쓰므로 범위가 필요 없다.
- DO: 의존을 추가·제거하는 PR 은 설명에 이유를 남긴다.
- 의존이 없으면 `dependencies` 필드를 두지 않는다.

## 범위 밖

- 등록 형식(필수 파일·필드)은 `tools/validate` 가 CI 로 집행한다. 이 문서에 복제하지 않는다.
- 공유 스크립트(`common/`)를 플러그인이 호출하는 것은 플러그인 간 의존이 아니라 레포 내 공유 코드 결합이다. 이 규칙의 대상이 아니다.
