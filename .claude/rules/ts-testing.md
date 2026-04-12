---
paths:
  - "**/tests/**/*.test.ts"
---

# TypeScript 테스트 컨벤션

> 블랙박스 원칙. 인터페이스 계약을 검증하고 구현 내부에 결합하지 않는다.

## 원칙

1. **mock 함수 패턴**: `mockGit(overrides)` 형태의 팩토리 함수로 mock을 생성한다. 기본값은 성공 상태다
2. **블랙박스 검증**: command의 입력/출력만 검증하고, 내부 구현 순서에 결합하지 않는다
3. **호출 순서 검증**: 호출 순서가 계약의 일부인 경우에만 `calls` 배열로 추적한다
4. **에러 케이스 필수**: 각 사전 조건 실패와 핵심 에러 경로를 반드시 테스트한다

## DO

mock 팩토리로 서비스를 격리하고, describe/test 계층으로 케이스를 분류한다:

```typescript
import { describe, test, expect } from 'bun:test';
import { createBranchCommand } from '../../src/commands/branch';
import type { GitService } from '../../src/core/git';

// mock 팩토리: 기본값은 성공 상태, 필요한 메서드만 override
function mockGit(overrides: Partial<GitService> = {}): GitService {
  return {
    isInsideWorkTree: async () => true,
    getCurrentBranch: async () => 'main',
    detectDefaultBranch: async () => 'main',
    branchExists: async () => false,
    hasUncommittedChanges: async () => false,
    fetch: async () => {},
    checkout: async () => {},
    commit: async () => {},
    push: async () => {},
    pull: async () => {},
    addTracked: async () => {},
    ...overrides,
  };
}

describe('branch command', () => {
  describe('정상 동작', () => {
    test('output에 생성된 branchName과 baseBranch 반환', async () => {
      const cmd = createBranchCommand({ git: mockGit({
        branchExists: async (name, location) => name === 'main' && location === 'any',
      }) });
      const result = await cmd.run({ branchName: 'feat/login' });
      expect(result.ok).toBe(true);
      if (result.ok) expect(result.data.branchName).toBe('feat/login');
    });
  });

  describe('사전 조건 검증', () => {
    test('uncommitted 변경 있으면 ok: false 반환', async () => {
      const cmd = createBranchCommand({
        git: mockGit({ hasUncommittedChanges: async () => true }),
      });
      const result = await cmd.run({ branchName: 'feat/new' });
      expect(result.ok).toBe(false);
    });
  });
});
```

## DON'T

구현 내부를 import하거나 모든 메서드 호출 순서에 결합하지 않는다:

```typescript
// 내부 구현 직접 import — DON'T
import { exec } from '../../src/core/shell'; // 블랙박스 원칙 위반

// 성공 케이스에서 불필요한 호출 순서 검증 — DON'T
test('결과 확인', async () => {
  const calls: string[] = [];
  // 단순 결과 검증에서 호출 순서까지 검증하면 구현에 결합됨
  expect(calls).toEqual(['fetch', 'checkout:main', 'pull:main', 'checkout-create:feat/new']);
});

// mock 없이 실제 git 명령 실행 — DON'T
test('branch 생성', async () => {
  await exec(['git', 'checkout', '-b', 'test-branch']); // 격리 없음
});
```

## 체크리스트

- [ ] `mock*` 팩토리 함수에 성공 기본값이 설정되어 있는가?
- [ ] 테스트가 command의 `run()` 입출력만 검증하는가?
- [ ] 호출 순서 검증은 그것이 계약의 일부인 경우로 한정되는가?
- [ ] 입력 검증 실패, 사전 조건 실패, 핵심 에러 경로가 모두 커버되는가?
- [ ] `describe`/`test` 이름이 동작을 한국어로 명확히 서술하는가?
