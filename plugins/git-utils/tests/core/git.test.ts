import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createGitService } from '../../src/core/git';
import { exec } from '../../src/core/shell';

// ============================================================
// GitService — Integration Test (temp git repo)
// ============================================================

let tempDir: string;
let remoteDir: string;

async function shell(cmd: string[], cwd: string) {
  const result = await exec(cmd, { cwd });
  if (result.exitCode !== 0) throw new Error(`${cmd.join(' ')} failed: ${result.stderr}`);
  return result.stdout;
}

beforeEach(async () => {
  // bare remote repo 생성
  remoteDir = await mkdtemp(join(tmpdir(), 'git-utils-remote-'));
  await shell(['git', 'init', '--bare'], remoteDir);

  // local repo 생성 + initial commit + push
  tempDir = await mkdtemp(join(tmpdir(), 'git-utils-test-'));
  await shell(['git', 'init', '-b', 'main'], tempDir);
  await shell(['git', 'config', 'user.email', 'test@test.com'], tempDir);
  await shell(['git', 'config', 'user.name', 'Test'], tempDir);
  await shell(['git', 'config', 'commit.gpgsign', 'false'], tempDir);
  await writeFile(join(tempDir, 'README.md'), 'init');
  await shell(['git', 'add', '.'], tempDir);
  await shell(['git', 'commit', '-m', 'init'], tempDir);
  await shell(['git', 'remote', 'add', 'origin', remoteDir], tempDir);
  await shell(['git', 'push', '-u', 'origin', 'main'], tempDir);
  // set origin/HEAD (--auto doesn't work with local bare repos)
  await shell(['git', 'symbolic-ref', 'refs/remotes/origin/HEAD', 'refs/remotes/origin/main'], tempDir);
});

afterEach(async () => {
  await rm(tempDir, { recursive: true, force: true });
  await rm(remoteDir, { recursive: true, force: true });
});

describe('GitService', () => {
  describe('detectDefaultBranch', () => {
    test('origin/HEAD가 설정된 repo → 해당 브랜치 반환', async () => {
      const git = createGitService(tempDir);
      expect(await git.detectDefaultBranch()).toBe('main');
    });

    test('origin/HEAD 미설정, origin/main 존재 → "main" 반환', async () => {
      // origin/HEAD 삭제 → Method 1 실패 → Method 2(set-head --auto)도 local bare에선 실패 → Method 3 fallback
      await shell(['git', 'symbolic-ref', '--delete', 'refs/remotes/origin/HEAD'], tempDir);
      const git = createGitService(tempDir);
      expect(await git.detectDefaultBranch()).toBe('main');
    });

    test('origin/HEAD 미설정, origin/master만 존재 → "master" 반환', async () => {
      // master 기반 bare repo 재생성
      const masterRemote = await mkdtemp(join(tmpdir(), 'git-utils-master-'));
      await shell(['git', 'init', '--bare'], masterRemote);
      const masterLocal = await mkdtemp(join(tmpdir(), 'git-utils-mlocal-'));
      await shell(['git', 'init', '-b', 'master'], masterLocal);
      await shell(['git', 'config', 'user.email', 'test@test.com'], masterLocal);
      await shell(['git', 'config', 'user.name', 'Test'], masterLocal);
      await shell(['git', 'config', 'commit.gpgsign', 'false'], masterLocal);
      await writeFile(join(masterLocal, 'README.md'), 'init');
      await shell(['git', 'add', '.'], masterLocal);
      await shell(['git', 'commit', '-m', 'init'], masterLocal);
      await shell(['git', 'remote', 'add', 'origin', masterRemote], masterLocal);
      await shell(['git', 'push', '-u', 'origin', 'master'], masterLocal);

      // origin/HEAD를 설정하지 않은 상태에서 테스트
      const git = createGitService(masterLocal);
      expect(await git.detectDefaultBranch()).toBe('master');

      await rm(masterRemote, { recursive: true, force: true });
      await rm(masterLocal, { recursive: true, force: true });
    });

    test('origin/HEAD 미설정, origin/develop만 존재 → "develop" 반환', async () => {
      const devRemote = await mkdtemp(join(tmpdir(), 'git-utils-dev-'));
      await shell(['git', 'init', '--bare'], devRemote);
      const devLocal = await mkdtemp(join(tmpdir(), 'git-utils-dlocal-'));
      await shell(['git', 'init', '-b', 'develop'], devLocal);
      await shell(['git', 'config', 'user.email', 'test@test.com'], devLocal);
      await shell(['git', 'config', 'user.name', 'Test'], devLocal);
      await shell(['git', 'config', 'commit.gpgsign', 'false'], devLocal);
      await writeFile(join(devLocal, 'README.md'), 'init');
      await shell(['git', 'add', '.'], devLocal);
      await shell(['git', 'commit', '-m', 'init'], devLocal);
      await shell(['git', 'remote', 'add', 'origin', devRemote], devLocal);
      await shell(['git', 'push', '-u', 'origin', 'develop'], devLocal);

      const git = createGitService(devLocal);
      expect(await git.detectDefaultBranch()).toBe('develop');

      await rm(devRemote, { recursive: true, force: true });
      await rm(devLocal, { recursive: true, force: true });
    });

    test('remote 없는 repo → 에러 반환', async () => {
      const noRemote = await mkdtemp(join(tmpdir(), 'git-utils-noremote-'));
      await shell(['git', 'init'], noRemote);
      await shell(['git', 'config', 'user.email', 'test@test.com'], noRemote);
      await shell(['git', 'config', 'user.name', 'Test'], noRemote);
      await shell(['git', 'config', 'commit.gpgsign', 'false'], noRemote);
      await writeFile(join(noRemote, 'README.md'), 'init');
      await shell(['git', 'add', '.'], noRemote);
      await shell(['git', 'commit', '-m', 'init'], noRemote);

      const git = createGitService(noRemote);
      expect(git.detectDefaultBranch()).rejects.toThrow('Could not detect default branch');

      await rm(noRemote, { recursive: true, force: true });
    });
  });

  describe('getCurrentBranch', () => {
    test('일반 브랜치에서 → 브랜치 이름 반환', async () => {
      const git = createGitService(tempDir);
      expect(await git.getCurrentBranch()).toBe('main');
    });

    test('detached HEAD → 빈 문자열 반환', async () => {
      const { stdout: hash } = await exec(['git', 'rev-parse', 'HEAD'], { cwd: tempDir });
      await shell(['git', 'checkout', hash.trim()], tempDir);
      const git = createGitService(tempDir);
      expect(await git.getCurrentBranch()).toBe('');
    });
  });

  describe('branchExists', () => {
    test('로컬에 존재하는 브랜치 → true (local)', async () => {
      const git = createGitService(tempDir);
      expect(await git.branchExists('main', 'local')).toBe(true);
    });

    test('로컬에 없는 브랜치 → false (local)', async () => {
      const git = createGitService(tempDir);
      expect(await git.branchExists('nonexistent', 'local')).toBe(false);
    });

    test('리모트에 존재하는 브랜치 → true (remote)', async () => {
      const git = createGitService(tempDir);
      expect(await git.branchExists('main', 'remote')).toBe(true);
    });

    test('어디에도 없는 브랜치 → false (any)', async () => {
      const git = createGitService(tempDir);
      expect(await git.branchExists('nonexistent', 'any')).toBe(false);
    });
  });

  describe('isInsideWorkTree', () => {
    test('git repo 내부 → true', async () => {
      const git = createGitService(tempDir);
      expect(await git.isInsideWorkTree()).toBe(true);
    });

    test('git repo 외부 → false', async () => {
      const nonGit = await mkdtemp(join(tmpdir(), 'git-utils-nongit-'));
      const git = createGitService(nonGit);
      expect(await git.isInsideWorkTree()).toBe(false);
      await rm(nonGit, { recursive: true, force: true });
    });
  });

  describe('hasUncommittedChanges', () => {
    test('변경 없음 → false', async () => {
      const git = createGitService(tempDir);
      expect(await git.hasUncommittedChanges()).toBe(false);
    });

    test('unstaged 변경 있음 → true', async () => {
      await writeFile(join(tempDir, 'README.md'), 'modified');
      const git = createGitService(tempDir);
      expect(await git.hasUncommittedChanges()).toBe(true);
    });

    test('staged 변경 있음 → true', async () => {
      await writeFile(join(tempDir, 'README.md'), 'staged');
      await shell(['git', 'add', 'README.md'], tempDir);
      const git = createGitService(tempDir);
      expect(await git.hasUncommittedChanges()).toBe(true);
    });

    test('untracked 파일만 있음 → true', async () => {
      await writeFile(join(tempDir, 'new-file.txt'), 'new');
      const git = createGitService(tempDir);
      expect(await git.hasUncommittedChanges()).toBe(true);
    });
  });

  describe('getSpecialState', () => {
    test('정상 상태 → { rebase: false, merge: false, detached: false }', async () => {
      const git = createGitService(tempDir);
      expect(await git.getSpecialState()).toEqual({ rebase: false, merge: false, detached: false });
    });

    test('detached HEAD → { detached: true }', async () => {
      const { stdout: hash } = await exec(['git', 'rev-parse', 'HEAD'], { cwd: tempDir });
      await shell(['git', 'checkout', hash.trim()], tempDir);
      const git = createGitService(tempDir);
      const state = await git.getSpecialState();
      expect(state.detached).toBe(true);
    });
  });

  describe('addTracked', () => {
    test('tracked 파일의 변경사항만 staging', async () => {
      await writeFile(join(tempDir, 'README.md'), 'tracked change');
      const git = createGitService(tempDir);
      await git.addTracked();
      const { stdout } = await exec(['git', 'diff', '--cached', '--name-only'], { cwd: tempDir });
      expect(stdout).toContain('README.md');
    });

    test('untracked 파일은 staging하지 않음', async () => {
      await writeFile(join(tempDir, 'untracked.txt'), 'new file');
      await writeFile(join(tempDir, 'README.md'), 'tracked change');
      const git = createGitService(tempDir);
      await git.addTracked();
      const { stdout } = await exec(['git', 'diff', '--cached', '--name-only'], { cwd: tempDir });
      expect(stdout).not.toContain('untracked.txt');
      expect(stdout).toContain('README.md');
    });
  });
});
