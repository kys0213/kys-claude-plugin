// ============================================================
// GitService — Git 조작 인터페이스
// ============================================================
// detect-default-branch.sh, create-branch.sh 등에서 반복되던
// git 명령어 호출을 단일 서비스로 통합합니다.
//
// 리팩토링 포인트:
//   - detect-default-branch.sh 의 3단계 감지 로직 → detectDefaultBranch()
//   - 두 guard hook에서 중복된 상태 체크 → getSpecialState()
//   - create-branch.sh 의 fetch/checkout 로직 → fetch(), checkout() 등
// ============================================================

import type { GitSpecialState } from '../types';

export interface GitService {
  // -- Branch 감지 --
  /** 리포지토리의 기본 브랜치(main/master/develop)를 감지 */
  detectDefaultBranch(): Promise<string>;

  /** 현재 체크아웃된 브랜치 이름 (detached HEAD면 빈 문자열) */
  getCurrentBranch(): Promise<string>;

  /** 브랜치 존재 여부 확인 */
  branchExists(name: string, location: 'local' | 'remote' | 'any'): Promise<boolean>;

  // -- 상태 확인 --
  /** git 워킹 트리 내부인지 확인 */
  isInsideWorkTree(): Promise<boolean>;

  /** uncommitted 변경사항 존재 여부 */
  hasUncommittedChanges(): Promise<boolean>;

  /** rebase/merge/detached 등 특수 상태 확인 (guard 공통 로직) */
  getSpecialState(): Promise<GitSpecialState>;

  // -- 조작 --
  fetch(remote?: string): Promise<void>;
  checkout(branch: string, options?: { create?: boolean; track?: string }): Promise<void>;
  commit(message: string): Promise<void>;
  push(branch: string, options?: { setUpstream?: boolean }): Promise<void>;
  pull(branch: string): Promise<void>;
  addTracked(): Promise<void>;
}
