# Worker Task: {{TASK_ID}}

이 작업은 Team Claude 시스템에 의해 생성되었습니다.
아래 스펙에 따라 구현을 진행해주세요.

---

## Task 개요

**Task ID**: {{TASK_ID}}
**생성 시간**: {{CREATED_AT}}
**브랜치**: {{BRANCH}}

---

## 스펙

{{SPEC_CONTENT}}

---

## Contract (구현해야 할 인터페이스)

{{CONTRACT_CONTENT}}

---

## 완료 조건

{{COMPLETION_CRITERIA}}

---

## 테스트 항목

{{TEST_CASES}}

---

## 참고 자료

- Contract 정의: `.team-claude/contracts/`
- Flow 다이어그램: `.team-claude/specs/flows/`
- QA 테스트 케이스: `.team-claude/specs/qa/`

---

## 작업 지침

1. **구현**: Contract에 정의된 인터페이스를 구현합니다
2. **테스트**: 테스트 항목에 따라 단위 테스트를 작성합니다
3. **검증**: lint, typecheck, test가 모두 통과하는지 확인합니다
4. **커밋**: 의미 있는 커밋 메시지로 커밋합니다

## 완료 시

작업이 완료되면 Claude가 자동으로 완료를 감지합니다.
Stop hook이 Main Claude에 알림을 보냅니다.

## 피드백 확인

Main Claude로부터 피드백이 도착하면 `.claude/feedback.md` 파일에서 확인할 수 있습니다.

---

*이 파일은 Team Claude에 의해 자동 생성되었습니다.*
