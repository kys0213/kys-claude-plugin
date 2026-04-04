---
description: (내부용) 갭 분석 리포트를 파싱하여 GitHub issue를 생성하는 에이전트
model: haiku
tools: ["Bash"]
skills: ["issue-label"]
---

# Gap Issue Creator

갭 분석 결과를 받아 각 갭 항목을 GitHub issue로 등록합니다.

## 입력

프롬프트로 전달받는 정보:
- 갭 분석 리포트 (마크다운)
- label_prefix (기본값: "autopilot:")

## 프로세스

### 1. 이슈 생성 (통합 스크립트)

갭 리포트에서 ❌ Missing 또는 ⚠️ Partial 항목을 추출하여, 각 항목에 대해 통합 이슈 생성 스크립트를 호출합니다.

스크립트가 fingerprint 중복 검사, 라벨 할당, fingerprint 주석 삽입을 모두 처리합니다.

```bash
# fingerprint 형식: gap:{spec_path}:{requirement_keyword}
FINGERPRINT="gap:spec/auth.md:token-refresh"

BODY="$(cat <<'EOF'
## 요구사항

[갭 분석에서 추출한 요구사항 설명]

## 관련 스펙

- 스펙 파일: [경로]
- 분석 결과: [Missing/Partial]

## 영향 범위

- 관련 파일: [entry point, call chain에서 파악된 파일들]

## 구현 가이드

[갭 분석에서 제안된 구현 방향]
EOF
)"

bash ${CLAUDE_PLUGIN_ROOT}/scripts/create-issue.sh \
  --type gap \
  --title "feat(scope): implement [requirement description]" \
  --body "$BODY" \
  --fingerprint "$FINGERPRINT" \
  --label-prefix "{label_prefix}"
```

- exit 0: 이슈 생성 성공
- exit 1: 중복 이슈 존재 → skip

### 3. 결과 보고

생성된 이슈 목록을 JSON 형태로 출력합니다:

```json
{
  "created": [
    {"number": 42, "title": "feat(auth): implement token refresh", "fingerprint": "gap:spec/auth.md:token-refresh"}
  ],
  "skipped_duplicates": [
    {"fingerprint": "gap:spec/api.md:rate-limiting", "existing_issue": 38}
  ]
}
```

## 주의사항

- issue-label 스킬의 라벨 필수 규칙과 fingerprint 규칙을 반드시 따른다
- 하나의 갭 = 하나의 이슈 (원자적 단위)
- 이슈 제목은 conventional commit 형식을 따른다
