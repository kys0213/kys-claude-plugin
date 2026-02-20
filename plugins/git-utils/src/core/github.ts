// ============================================================
// GitHubService — GitHub CLI(gh) 래퍼 인터페이스 + 구현
// ============================================================
// create-pr.sh, unresolved-reviews.sh 에서 사용하는
// gh CLI 호출을 타입 안전한 인터페이스로 추상화합니다.
// ============================================================

import { readFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';
import type { ReviewThread } from '../types';
import { exec, execOrThrow } from './shell';

/** ~/.git-workflow-env 에서 GH_HOST를 읽어 반환 */
function loadGhHost(): string | undefined {
  try {
    const envPath = join(homedir(), '.git-workflow-env');
    const content = readFileSync(envPath, 'utf-8');
    const match = content.match(/^export\s+GH_HOST="(.+)"/m);
    return match?.[1] || undefined;
  } catch {
    return undefined;
  }
}

export interface GitHubService {
  /** gh auth status 확인 */
  isAuthenticated(): Promise<boolean>;

  /** PR 생성 — gh pr create */
  createPr(options: {
    base: string;
    title: string;
    body: string;
  }): Promise<string>; // returns PR URL

  /** PR 리뷰 쓰레드 조회 — gh api graphql */
  getReviewThreads(prNumber: number): Promise<{
    prTitle: string;
    prUrl: string;
    threads: ReviewThread[];
  }>;

  /** 현재 브랜치의 PR 번호 자동 감지 */
  detectCurrentPrNumber(): Promise<number | null>;
}

// -- GraphQL query for review threads --

const REVIEW_THREADS_QUERY = `
query($owner: String!, $repo: String!, $number: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $number) {
      title
      url
      reviewThreads(first: 100) {
        nodes {
          isResolved
          isOutdated
          path
          line
          comments(first: 100) {
            nodes {
              author { login }
              body
              createdAt
              url
            }
          }
        }
      }
    }
  }
}
`;

export function createGitHubService(cwd?: string): GitHubService {
  const ghHost = loadGhHost();
  const env = ghHost ? { GH_HOST: ghHost } : undefined;
  const opts = { ...(cwd ? { cwd } : {}), ...(env ? { env } : {}) };
  const execOpts = Object.keys(opts).length > 0 ? opts : undefined;

  async function gh(...args: string[]): Promise<string> {
    return execOrThrow(['gh', ...args], execOpts);
  }

  async function ghSafe(...args: string[]): Promise<{ stdout: string; exitCode: number }> {
    const result = await exec(['gh', ...args], execOpts);
    return { stdout: result.stdout, exitCode: result.exitCode };
  }

  return {
    async isAuthenticated(): Promise<boolean> {
      const { exitCode } = await ghSafe('auth', 'status');
      return exitCode === 0;
    },

    async createPr({ base, title, body }): Promise<string> {
      const args = ['pr', 'create', '--base', base, '--title', title, '--body', body];
      const url = await gh(...args);
      return url.trim();
    },

    async getReviewThreads(prNumber: number) {
      // Get repo owner/name
      const repoInfo = await gh('repo', 'view', '--json', 'owner,name');
      const { owner: { login: owner }, name: repo } = JSON.parse(repoInfo);

      const result = await gh(
        'api', 'graphql',
        '-f', `query=${REVIEW_THREADS_QUERY}`,
        '-f', `owner=${owner}`,
        '-f', `repo=${repo}`,
        '-F', `number=${prNumber}`,
      );

      const data = JSON.parse(result);
      const pr = data.data.repository.pullRequest;

      const threads: ReviewThread[] = pr.reviewThreads.nodes.map((node: any) => ({
        isResolved: node.isResolved,
        isOutdated: node.isOutdated,
        path: node.path,
        line: node.line ?? 0,
        comments: node.comments.nodes.map((c: any) => ({
          author: c.author?.login ?? 'ghost',
          body: c.body,
          createdAt: c.createdAt,
          url: c.url,
        })),
      }));

      return { prTitle: pr.title, prUrl: pr.url, threads };
    },

    async detectCurrentPrNumber(): Promise<number | null> {
      const { stdout, exitCode } = await ghSafe('pr', 'view', '--json', 'number');
      if (exitCode !== 0) return null;
      try {
        const { number } = JSON.parse(stdout);
        return typeof number === 'number' ? number : null;
      } catch {
        return null;
      }
    },
  };
}
