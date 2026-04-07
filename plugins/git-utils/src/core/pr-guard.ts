// ============================================================
// PrGuardService — PR 중복 생성 방지 Guard
// ============================================================
// gh pr create 실행 전 현재 브랜치에 열린 PR이 있는지 검증합니다.
// 열린 PR이 있으면 차단하고, 네트워크 오류 시 안전 모드(허용)로 동작합니다.
// ============================================================

import type { PrGuardInput, PrGuardOutput } from '../types';
import type { GitHubService } from './github';

export interface PrGuardService {
  check(input: PrGuardInput): Promise<PrGuardOutput>;
}

// Note: 이 패턴은 Claude의 Bash 도구가 생성하는 표준 형태의 gh CLI 명령을 대상으로 합니다.
// `gh --repo owner/repo pr create` 같은 변형은 매칭하지 않습니다.
const GH_PR_CREATE_PATTERN = /\bgh\s+pr\s+create\b/;

export function createPrGuardService(github: GitHubService): PrGuardService {
  return {
    async check(input: PrGuardInput): Promise<PrGuardOutput> {
      const pass = (reason?: string): PrGuardOutput => ({
        allowed: true,
        reason,
      });

      // toolCommand가 있으면 gh pr create 패턴인지 확인
      if (input.toolCommand !== undefined) {
        if (!GH_PR_CREATE_PATTERN.test(input.toolCommand)) {
          return pass('not a gh pr create command');
        }
      }

      // 현재 브랜치에 열린 PR이 있는지 확인
      let prNumber: number | null;
      try {
        prNumber = await github.detectCurrentPrNumber();
      } catch {
        return pass('could not check existing PR (safe mode)');
      }

      if (prNumber === null) {
        return pass();
      }

      // 열린 PR이 있으면 차단
      return {
        allowed: false,
        prNumber,
        reason: [
          `[PR Guard] 현재 브랜치에 열린 PR이 있습니다.`,
          ``,
          `기존 PR:`,
          `  번호: #${prNumber}`,
          ``,
          `새로운 PR을 생성하려면:`,
          `  1. 기존 PR을 머지하거나 닫기`,
          `  2. 기본 브랜치로 동기화`,
          `  3. 새 브랜치 생성 후 다시 시도`,
          ``,
          `기존 PR에 변경사항을 추가하려면:`,
          `  - git push만 실행하세요`,
        ].join('\n'),
      };
    },
  };
}
