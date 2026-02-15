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

  // 각 command 모듈은 Step 3(TDD 구현)에서 연결됩니다.
  // 현재는 라우팅 구조만 정의합니다.
  switch (command) {
    case 'commit':
      // → src/commands/commit.ts
      break;
    case 'branch':
      // → src/commands/branch.ts
      break;
    case 'pr':
      // → src/commands/pr.ts
      break;
    case 'reviews':
      // → src/commands/reviews.ts
      break;
    case 'guard':
      // → src/commands/guard.ts
      break;
    case 'hook':
      // → src/commands/hook.ts
      break;
  }

  console.log(`[stub] command=${command}, args=`, parsed);
}

// bun에서 직접 실행할 때만 main() 호출 (import 시에는 실행 안 함)
if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
