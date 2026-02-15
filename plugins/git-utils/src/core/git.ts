// ============================================================
// GitService — Git 조작 구현
// ============================================================

import type { GitSpecialState } from '../types';
import { exec, execOrThrow } from './shell';

export interface GitService {
  detectDefaultBranch(): Promise<string>;
  getCurrentBranch(): Promise<string>;
  branchExists(name: string, location: 'local' | 'remote' | 'any'): Promise<boolean>;
  isInsideWorkTree(): Promise<boolean>;
  hasUncommittedChanges(): Promise<boolean>;
  getSpecialState(): Promise<GitSpecialState>;
  fetch(remote?: string): Promise<void>;
  checkout(branch: string, options?: { create?: boolean; track?: string }): Promise<void>;
  commit(message: string): Promise<void>;
  push(branch: string, options?: { setUpstream?: boolean }): Promise<void>;
  pull(branch: string): Promise<void>;
  addTracked(): Promise<void>;
}

export function createGitService(cwd?: string): GitService {
  const opts = cwd ? { cwd } : undefined;

  async function git(...args: string[]): Promise<string> {
    return execOrThrow(['git', ...args], opts);
  }

  async function gitSafe(...args: string[]): Promise<{ stdout: string; exitCode: number }> {
    const result = await exec(['git', ...args], opts);
    return { stdout: result.stdout, exitCode: result.exitCode };
  }

  return {
    async detectDefaultBranch(): Promise<string> {
      // Method 1: cached origin/HEAD
      const { stdout: head, exitCode: headExit } = await gitSafe(
        'symbolic-ref', 'refs/remotes/origin/HEAD',
      );
      if (headExit === 0 && head) {
        return head.replace('refs/remotes/origin/', '');
      }

      // Method 2: auto-detect from remote
      await exec(['git', 'remote', 'set-head', 'origin', '--auto'], opts);
      const { stdout: head2, exitCode: headExit2 } = await gitSafe(
        'symbolic-ref', 'refs/remotes/origin/HEAD',
      );
      if (headExit2 === 0 && head2) {
        return head2.replace('refs/remotes/origin/', '');
      }

      // Method 3: fallback to common names
      for (const name of ['main', 'develop', 'master']) {
        const { exitCode } = await gitSafe(
          'show-ref', '--verify', '--quiet', `refs/remotes/origin/${name}`,
        );
        if (exitCode === 0) return name;
      }

      throw new Error('Could not detect default branch. Make sure you have a remote configured.');
    },

    async getCurrentBranch(): Promise<string> {
      const { stdout, exitCode } = await gitSafe('branch', '--show-current');
      if (exitCode !== 0) return '';
      return stdout;
    },

    async branchExists(name: string, location: 'local' | 'remote' | 'any'): Promise<boolean> {
      if (location === 'local' || location === 'any') {
        const { exitCode } = await gitSafe('show-ref', '--verify', '--quiet', `refs/heads/${name}`);
        if (exitCode === 0) return true;
      }
      if (location === 'remote' || location === 'any') {
        const { exitCode } = await gitSafe('show-ref', '--verify', '--quiet', `refs/remotes/origin/${name}`);
        if (exitCode === 0) return true;
      }
      return false;
    },

    async isInsideWorkTree(): Promise<boolean> {
      const { exitCode } = await gitSafe('rev-parse', '--is-inside-work-tree');
      return exitCode === 0;
    },

    async hasUncommittedChanges(): Promise<boolean> {
      const { stdout } = await gitSafe('status', '--porcelain');
      return stdout.length > 0;
    },

    async getSpecialState(): Promise<GitSpecialState> {
      const { stdout: gitDir } = await gitSafe('rev-parse', '--git-dir');

      const rebase = await (async () => {
        try {
          const { existsSync } = await import('node:fs');
          return existsSync(`${gitDir}/rebase-merge`) || existsSync(`${gitDir}/rebase-apply`);
        } catch {
          return false;
        }
      })();

      const merge = await (async () => {
        try {
          const { existsSync } = await import('node:fs');
          return existsSync(`${gitDir}/MERGE_HEAD`);
        } catch {
          return false;
        }
      })();

      const branch = await this.getCurrentBranch();
      const detached = branch === '';

      return { rebase, merge, detached };
    },

    async fetch(remote = 'origin'): Promise<void> {
      await exec(['git', 'fetch', remote, '--prune'], opts);
    },

    async checkout(branch: string, options?: { create?: boolean; track?: string }): Promise<void> {
      const args = ['checkout'];
      if (options?.create) args.push('-b');
      args.push(branch);
      if (options?.track) args.push('--track', options.track);
      await git(...args);
    },

    async commit(message: string): Promise<void> {
      await git('commit', '-m', message);
    },

    async push(branch: string, options?: { setUpstream?: boolean }): Promise<void> {
      const args = ['push'];
      if (options?.setUpstream) args.push('-u');
      args.push('origin', branch);
      await git(...args);
    },

    async pull(branch: string): Promise<void> {
      await exec(['git', 'pull', 'origin', branch], opts);
    },

    async addTracked(): Promise<void> {
      await git('add', '-u');
    },
  };
}
