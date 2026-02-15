#!/usr/bin/env bun
// ============================================================
// git-utils CLI — Entry Point
// ============================================================
// 기존 9개 스크립트를 하나의 CLI로 통합합니다.
//
// Install:
//   /setup 실행 시 bun build --compile → ~/.local/bin/git-utils
//
// Usage:
//   git-utils <command> [subcommand] [args...] [options...]
//   git-utils --version
//
// Commands:
//   commit   <type> <description> [--scope=<s>] [--body=<b>] [--skip-add]
//   branch   <branch-name> [--base=<branch>]
//   pr       <title> [--description=<d>]
//   reviews  [pr-number]
//   guard    <write|commit> --project-dir=<p> --create-branch-script=<s> [--default-branch=<b>]
//   hook     <register|unregister|list> [args...] [--timeout=<n>] [--project-dir=<p>]
// ============================================================

import { createGitService, createJiraService, createGitHubService, createGuardService } from './core';
import { createCommitCommand } from './commands/commit';
import { createBranchCommand } from './commands/branch';
import { createPrCommand } from './commands/pr';
import { createReviewsCommand } from './commands/reviews';
import { createHookCommand } from './commands/hook';
import type { CommitType, GuardTarget } from './types';
import { readFile } from 'node:fs/promises';

/** plugin.json과 동기화 — build 시점에 bake됩니다 */
export const VERSION = '3.0.0-alpha.0';

const COMMANDS = ['commit', 'branch', 'pr', 'reviews', 'guard', 'hook'] as const;
type CommandName = (typeof COMMANDS)[number];

// -- Args parser (lightweight, no deps) --

export interface ParsedArgs {
  positional: string[];
  flags: Record<string, string | boolean>;
}

export function parseArgs(argv: string[]): ParsedArgs {
  const positional: string[] = [];
  const flags: Record<string, string | boolean> = {};

  for (const arg of argv) {
    if (arg.startsWith('--')) {
      const eqIndex = arg.indexOf('=');
      if (eqIndex !== -1) {
        flags[arg.slice(2, eqIndex)] = arg.slice(eqIndex + 1);
      } else {
        flags[arg.slice(2)] = true;
      }
    } else {
      positional.push(arg);
    }
  }

  return { positional, flags };
}

// -- Usage --

function printUsage(): void {
  console.log(`
git-utils — Git workflow automation CLI

Usage:
  git-utils <command> [args...] [options...]

Commands:
  commit    Smart commit with Jira ticket detection
  branch    Create a new branch from base branch
  pr        Create a Pull Request
  reviews   Query unresolved PR review threads
  guard     Default branch guard (Claude hook)
  hook      Manage Claude Code hooks in settings.json

Run 'git-utils <command> --help' for command-specific usage.
  `.trim());
}

// -- Result output helper --

function output(result: { ok: boolean; data?: any; error?: string }): void {
  if (result.ok) {
    console.log(JSON.stringify(result.data, null, 2));
  } else {
    console.error(`Error: ${result.error}`);
    process.exit(1);
  }
}

// -- Subcommand dispatch --

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    printUsage();
    process.exit(0);
  }

  if (args[0] === '--version' || args[0] === '-v') {
    console.log(`git-utils v${VERSION}`);
    process.exit(0);
  }

  const command = args[0] as CommandName;
  if (!COMMANDS.includes(command)) {
    console.error(`Unknown command: ${command}`);
    printUsage();
    process.exit(1);
  }

  const parsed = parseArgs(args.slice(1));

  // Lazy service creation (only when needed)
  const git = createGitService();
  const jira = createJiraService();
  const github = createGitHubService();

  switch (command) {
    case 'commit': {
      const cmd = createCommitCommand({ git, jira });
      const result = await cmd.run({
        type: parsed.positional[0] as CommitType,
        description: parsed.positional[1] || '',
        scope: parsed.flags['scope'] as string | undefined,
        body: parsed.flags['body'] as string | undefined,
        skipAdd: parsed.flags['skip-add'] === true,
      });
      output(result);
      break;
    }

    case 'branch': {
      const cmd = createBranchCommand({ git });
      const result = await cmd.run({
        branchName: parsed.positional[0] || '',
        baseBranch: parsed.flags['base'] as string | undefined,
      });
      output(result);
      break;
    }

    case 'pr': {
      const cmd = createPrCommand({ git, jira, github });
      const result = await cmd.run({
        title: parsed.positional[0] || '',
        description: parsed.flags['description'] as string | undefined,
      });
      output(result);
      break;
    }

    case 'reviews': {
      const cmd = createReviewsCommand({ github });
      const prNum = parsed.positional[0] ? parseInt(parsed.positional[0], 10) : undefined;
      const result = await cmd.run({ prNumber: prNum });
      output(result);
      break;
    }

    case 'guard': {
      const guard = createGuardService(git);
      const target = parsed.positional[0] as GuardTarget;
      if (!target || !['write', 'commit'].includes(target)) {
        console.error('Usage: git-utils guard <write|commit> --project-dir=<p> --create-branch-script=<s>');
        process.exit(1);
      }

      // Read stdin for hook JSON (Claude hook provides tool input)
      let toolCommand: string | undefined;
      if (target === 'commit') {
        try {
          const stdin = await readFile('/dev/stdin', 'utf-8');
          const hookInput = JSON.parse(stdin);
          toolCommand = hookInput?.tool_input?.command;
        } catch {
          // stdin not available or not JSON
        }
      }

      const result = await guard.check({
        target,
        projectDir: (parsed.flags['project-dir'] as string) || process.cwd(),
        createBranchScript: (parsed.flags['create-branch-script'] as string) || 'git-utils branch',
        defaultBranch: parsed.flags['default-branch'] as string | undefined,
        toolCommand,
      });

      if (!result.allowed) {
        console.error(result.reason);
        process.exit(2);
      }
      break;
    }

    case 'hook': {
      const hookCmd = createHookCommand({
        fs: {
          readFile: async (p) => Bun.file(p).text(),
          writeFile: async (p, c) => { await Bun.write(p, c); },
          exists: async (p) => {
            const { existsSync } = await import('node:fs');
            return existsSync(p);
          },
          mkdir: async (p) => {
            const { mkdirSync } = await import('node:fs');
            mkdirSync(p, { recursive: true });
          },
        },
      });

      const sub = parsed.positional[0];
      const projectDir = parsed.flags['project-dir'] as string | undefined;

      if (sub === 'register') {
        const result = await hookCmd.register({
          hookType: parsed.positional[1] || '',
          matcher: parsed.positional[2] || '',
          command: parsed.positional[3] || '',
          timeout: parsed.flags['timeout'] ? Number(parsed.flags['timeout']) : undefined,
          projectDir,
        });
        output(result);
      } else if (sub === 'unregister') {
        const result = await hookCmd.unregister({
          hookType: parsed.positional[1] || '',
          command: parsed.positional[2] || '',
          projectDir,
        });
        output(result);
      } else if (sub === 'list') {
        const result = await hookCmd.list({
          hookType: parsed.positional[1] || undefined,
          projectDir,
        });
        output(result);
      } else {
        console.error('Usage: git-utils hook <register|unregister|list> [args...]');
        process.exit(1);
      }
      break;
    }
  }
}

// bun에서 직접 실행할 때만 main() 호출 (import 시에는 실행 안 함)
if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
