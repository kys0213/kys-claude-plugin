---
paths:
  - "**/src/core/*.ts"
---

# TypeScript Core 컨벤션

> 서비스 인터페이스와 구현을 같은 파일에 정의. 추상화 먼저, shell 래핑은 내부에 캡슐화한다.

## 원칙

1. **인터페이스 우선**: 파일 상단에 `export interface *Service`를 먼저 선언한다
2. **팩토리로 구현 제공**: `export function create*Service()` 팩토리로 인터페이스를 구현해 반환한다
3. **shell 래핑 캡슐화**: `exec`, `execOrThrow`는 core 내부에서만 사용하고 외부에 노출하지 않는다
4. **에러 전파 전략**: 복구 불가 오류는 throw, 상태 확인은 boolean/null 반환으로 구분한다

## DO

인터페이스와 팩토리 구현을 같은 파일에 두고, shell 호출은 내부 함수로 감싼다:

```typescript
import type { GitSpecialState } from '../types';
import { exec, execOrThrow } from './shell';

// 1. 인터페이스 먼저 export
export interface GitService {
  getCurrentBranch(): Promise<string>;
  hasUncommittedChanges(): Promise<boolean>;
  checkout(branch: string, options?: { create?: boolean; track?: string }): Promise<void>;
}

// 2. 팩토리 함수로 구현 제공
export function createGitService(cwd?: string): GitService {
  const opts = cwd ? { cwd } : undefined;

  // 내부 helper — 외부에 노출하지 않음
  async function git(...args: string[]): Promise<string> {
    return execOrThrow(['git', ...args], opts);
  }

  return {
    async getCurrentBranch(): Promise<string> {
      const { stdout, exitCode } = await exec(['git', 'branch', '--show-current'], opts);
      if (exitCode !== 0) return '';
      return stdout;
    },
    // ...
  };
}
```

## DON'T

인터페이스 없이 구체 클래스만 export하거나, shell을 command 레이어에 직접 노출하지 않는다:

```typescript
// 인터페이스 없이 클래스만 export — DON'T (mock 불가)
export class GitService {
  async getCurrentBranch() { /* ... */ }
}

// exec를 core 밖으로 re-export — DON'T
export { exec } from './shell'; // command가 shell을 직접 쓰게 되는 경로 차단

// 단일 파일에 모든 서비스 혼재 — DON'T
// git.ts에 JiraService 인터페이스 선언은 jira.ts에 분리
```

## 체크리스트

- [ ] 파일 상단에 `export interface *Service`가 선언되어 있는가?
- [ ] `create*Service()` 팩토리 함수가 인터페이스 타입을 반환하는가?
- [ ] `exec`/`execOrThrow`는 해당 core 파일 내부에서만 사용하는가?
- [ ] 하나의 파일에 하나의 서비스 인터페이스만 정의했는가?
- [ ] 에러 처리 전략(throw vs boolean)이 일관적인가?
