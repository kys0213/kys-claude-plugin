---
name: issue-report
description: 분석 에이전트가 발견한 이슈를 보고하는 통일 리포트 형식
version: 1.0.0
---

# Issue Report Spec

분석 에이전트(보안, 성능, 커버리지 등)가 발견한 이슈를 보고할 때 사용하는 통일 형식입니다.

## 심각도 (Severity)

| 레벨 | 아이콘 | 기준 |
|------|--------|------|
| critical | 🔴 | 기능 장애, 데이터 유실, 보안 침해 등 즉시 조치 필요 |
| warning | 🟡 | 잠재적 문제. 현재 동작하지만 조건에 따라 장애 가능 |
| info | 🟢 | 개선하면 좋으나 현재 문제 없음 |

## 이슈 항목 구조 (Issue Entry)

각 이슈는 다음 필드를 가집니다:

```json
{
  "severity": "critical | warning | info",
  "category": "분석 영역 내 하위 카테고리",
  "title": "이슈 한줄 요약",
  "location": "file/path.rs:42",
  "description": "현재 코드가 어떤 상태인지 서술",
  "suggestion": "구체적 개선 방법",
  "impact": "개선 시 기대 효과 (선택)"
}
```

### 필드 작성 가이드

- **category**: 분석 관점에 따라 자유롭게 지정 (예: Injection, N+1, 미구현 등)
- **title**: 20자 이내. 무엇이 문제인지 명확하게
- **location**: `파일경로:라인번호` 형식. 여러 위치면 쉼표 구분
- **description**: 코드의 현재 상태를 기술. "~하고 있다" 서술
- **suggestion**: 구체적 수정 방법. "~로 변경한다" 명령형 서술
- **impact**: 정량적 추정이 가능하면 포함 (선택 필드)

## 마크다운 출력 형식

### 요약 테이블

```markdown
| 심각도 | 카테고리 | 항목 | 위치 | 설명 |
|--------|---------|------|------|------|
| 🔴 | Injection | SQL Injection | db/query.rs:23 | 파라미터 바인딩 미사용 |
| 🟡 | 메모리 | 불필요한 clone | service.rs:15 | &str 참조로 대체 가능 |
| 🟢 | 복잡도 | O(n²) 루프 | util.rs:88 | 현재 규모에서는 무해 |
```

### 심각도별 상세

```markdown
### 🔴 Critical

#### 1. [title] (`location`)
- **현재**: [description]
- **개선**: [suggestion]
- **효과**: [impact]

### 🟡 Warning

#### 1. [title] (`location`)
- **현재**: [description]
- **개선**: [suggestion]

### 🟢 Info

#### 1. [title] (`location`)
- **현재**: [description]
- **개선**: [suggestion]
```

## 통계 요약

리포트 마지막에 반드시 통계를 포함합니다:

```markdown
### 통계
- 🔴 Critical: N건
- 🟡 Warning: N건
- 🟢 Info: N건
- **합계**: N건
```
