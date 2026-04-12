---
paths:
  - "**/src/types.ts"
---

# TypeScript Types 컨벤션

> 모든 command와 core가 공유하는 계약. 구현 없이 타입 선언만 담는다.

## 원칙

1. **중앙 집중**: 공용 Input/Output 타입은 모두 `types.ts` 한 곳에 정의한다
2. **Result 패턴**: 모든 command 반환값은 `Result<T>` 유니언 타입을 사용한다
3. **Input/Output 쌍**: 각 command마다 `*Input`과 `*Output` 인터페이스를 쌍으로 정의한다
4. **const assertion 활용**: 리터럴 값 집합은 `as const` 배열로 정의하고 타입을 파생한다

## DO

`Result<T>` 패턴과 Input/Output 쌍을 일관되게 사용한다:

```typescript
// 공용 Result 타입
export type Result<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

// 공용 Command 인터페이스
export interface Command<TInput, TOutput> {
  readonly name: string;
  readonly description: string;
  run(input: TInput): Promise<Result<TOutput>>;
}

// const assertion으로 리터럴 집합 정의
export const COMMIT_TYPES = [
  'feat', 'fix', 'docs', 'style', 'refactor', 'test', 'chore', 'perf',
] as const;
export type CommitType = (typeof COMMIT_TYPES)[number];

// Input/Output 쌍
export interface CommitInput {
  type: CommitType;
  description: string;
  scope?: string;
  body?: string;
  skipAdd?: boolean;
}

export interface CommitOutput {
  subject: string;
  jiraTicket?: string;
}
```

## DON'T

구현 코드를 types.ts에 두거나, 인라인 타입으로 계약을 분산하지 않는다:

```typescript
// types.ts에 구현 포함 — DON'T
export function buildSubject(input: CommitInput): string { /* ... */ }

// command 파일 내 인라인 타입 — DON'T
// commit.ts 안에서 직접 interface CommitInput 선언 (types.ts에 있어야 함)

// any 타입 사용 — DON'T
export type Result<T> = { ok: boolean; data?: any; error?: any };
```

## 체크리스트

- [ ] 새 command를 추가할 때 `*Input`과 `*Output` 인터페이스를 types.ts에 추가했는가?
- [ ] 모든 command 반환값이 `Result<T>` 타입을 사용하는가?
- [ ] 리터럴 값 집합은 `as const` 배열로 정의하고 타입을 파생했는가?
- [ ] types.ts에 함수 구현이 없는가?
- [ ] 공용 타입을 각 파일에서 중복 선언하지 않았는가?
