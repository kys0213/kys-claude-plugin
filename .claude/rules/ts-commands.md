---
paths:
  - "**/src/commands/*.ts"
---

# TypeScript Command 컨벤션

> CLI 진입점. 입력 파싱과 core 위임만 담당하며 비즈니스 로직을 포함하지 않는다.

## 원칙

1. **의존성 주입**: `Deps` 인터페이스로 의존하는 서비스를 팩토리 함수 파라미터로 받는다
2. **Result 타입 반환**: 모든 command는 `Promise<Result<TOutput>>`을 반환한다
3. **입력 유효성 검사**: core를 호출하기 전에 입력값의 기본 유효성을 검사하고 `ok: false`를 반환한다
4. **로직 없음**: 비즈니스 판단(브랜치 존재 여부, 티켓 감지 등)은 core에 위임한다

## DO

팩토리 함수 패턴으로 의존성을 주입하고, 입력 유효성 검사 후 core에 위임한다:

```typescript
import type { Result, BranchInput, BranchOutput } from '../types';
import type { GitService } from '../core/git';

export interface BranchDeps {
  git: GitService;
}

export function createBranchCommand(deps: BranchDeps) {
  return {
    name: 'branch',
    description: 'Create a new branch from base branch',

    async run(input: BranchInput): Promise<Result<BranchOutput>> {
      // 1. 입력 유효성 검사 (즉시 반환)
      if (!input.branchName || input.branchName.trim() === '') {
        return { ok: false, error: 'Branch name is required' };
      }

      // 2. core에 모든 비즈니스 로직 위임
      if (await deps.git.hasUncommittedChanges()) {
        return { ok: false, error: 'Uncommitted changes detected.' };
      }

      // 3. 성공 결과 반환
      const baseBranch = await deps.git.detectDefaultBranch();
      return { ok: true, data: { branchName: input.branchName, baseBranch } };
    },
  };
}
```

## DON'T

command 내부에서 직접 shell 실행하거나 비즈니스 로직을 구현하지 않는다:

```typescript
// command가 직접 shell 실행 — DON'T
export function createBranchCommand() {
  return {
    async run(input: BranchInput) {
      const result = await exec(['git', 'status', '--porcelain']); // core 책임
      if (result.stdout.length > 0) { /* ... */ }
    },
  };
}

// Deps 없이 concrete 구현에 직접 의존 — DON'T
import { exec } from '../core/shell'; // command는 shell을 직접 import하지 않는다
```

## 체크리스트

- [ ] `create*Command(deps: *Deps)` 팩토리 함수 패턴을 사용했는가?
- [ ] 반환 타입이 `Promise<Result<TOutput>>`인가?
- [ ] 입력 유효성 검사는 맨 앞에, core 호출 전에 위치하는가?
- [ ] shell, DB 등 인프라를 command에서 직접 import하지 않는가?
- [ ] Deps 인터페이스에 실제 필요한 서비스만 선언했는가?
