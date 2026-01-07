#!/usr/bin/env node

import { resolve } from 'path';
import chalk from 'chalk';
import { validateSpecs } from './validators/spec-validator.js';
import { validatePaths } from './validators/path-validator.js';
import { validateVersions } from './validators/version-validator.js';

const args = process.argv.slice(2);
const repoRoot = resolve(args.find(a => !a.startsWith('--')) || process.cwd());

const options = {
  specsOnly: args.includes('--specs-only'),
  pathsOnly: args.includes('--paths-only'),
  versionsOnly: args.includes('--versions-only'),
  verbose: args.includes('--verbose') || args.includes('-v'),
};

// 특정 옵션이 없으면 전체 실행
const runAll = !options.specsOnly && !options.pathsOnly && !options.versionsOnly;

console.log(chalk.bold('\n========================================'));
console.log(chalk.bold('  Claude Code Plugin Validator'));
console.log(chalk.bold('========================================\n'));
console.log(chalk.gray(`Repository: ${repoRoot}\n`));

let totalPassed = 0;
let totalFailed = 0;

async function main() {
  try {
    // 1. 스펙 검증
    if (runAll || options.specsOnly) {
      console.log(chalk.cyan.bold('📋 [1/3] Validating Specs...'));
      console.log(chalk.gray('    Checking plugin.json, SKILL.md, agents, commands\n'));

      const specResults = await validateSpecs(repoRoot);
      printResults(specResults, options.verbose);
      totalPassed += specResults.passed.length;
      totalFailed += specResults.failed.length;
      console.log();
    }

    // 2. 경로 검증
    if (runAll || options.pathsOnly) {
      console.log(chalk.cyan.bold('📁 [2/3] Validating Paths (AST-based)...'));
      console.log(chalk.gray('    Checking file references, script paths\n'));

      const pathResults = await validatePaths(repoRoot);
      printResults(pathResults, options.verbose);
      totalPassed += pathResults.passed.length;
      totalFailed += pathResults.failed.length;
      console.log();
    }

    // 3. 버전 검증
    if (runAll || options.versionsOnly) {
      console.log(chalk.cyan.bold('🏷️  [3/3] Validating Versions...'));
      console.log(chalk.gray('    Checking semver format, consistency\n'));

      const versionResults = await validateVersions(repoRoot);
      printResults(versionResults, options.verbose);
      totalPassed += versionResults.passed.length;
      totalFailed += versionResults.failed.length;
      console.log();
    }

    // 결과 요약
    console.log(chalk.bold('========================================'));
    console.log(chalk.bold('  Summary'));
    console.log(chalk.bold('========================================'));
    console.log(chalk.green(`  ✓ Passed: ${totalPassed}`));
    console.log(chalk.red(`  ✗ Failed: ${totalFailed}`));
    console.log();

    if (totalFailed > 0) {
      console.log(chalk.red.bold('❌ Validation FAILED\n'));
      process.exit(1);
    } else {
      console.log(chalk.green.bold('✅ Validation PASSED\n'));
      process.exit(0);
    }

  } catch (error) {
    console.error(chalk.red(`\n❌ Error: ${error.message}\n`));
    if (options.verbose) {
      console.error(error.stack);
    }
    process.exit(1);
  }
}

function printResults(results, verbose) {
  // 실패한 항목 출력
  for (const result of results.failed) {
    const shortPath = result.file.replace(repoRoot, '.');
    console.log(chalk.red(`  ✗ ${shortPath}`));
    console.log(chalk.gray(`    Type: ${result.type}`));

    if (result.errors && result.errors.length > 0) {
      for (const error of result.errors) {
        console.log(chalk.red(`    → ${error}`));
      }
    } else if (result.error) {
      console.log(chalk.red(`    → ${result.error}`));
    }

    if (result.referencedPath) {
      console.log(chalk.gray(`    Referenced: ${result.referencedPath}`));
    }
    if (result.resolvedPath && verbose) {
      console.log(chalk.gray(`    Resolved: ${result.resolvedPath}`));
    }
    console.log();
  }

  // 성공한 항목 (verbose 모드에서만)
  if (verbose) {
    for (const result of results.passed) {
      const shortPath = result.file.replace(repoRoot, '.');
      console.log(chalk.green(`  ✓ ${shortPath}`));
      console.log(chalk.gray(`    Type: ${result.type}`));
      if (result.referencedPath) {
        console.log(chalk.gray(`    Referenced: ${result.referencedPath}`));
      }
    }
  } else if (results.passed.length > 0) {
    console.log(chalk.green(`  ✓ ${results.passed.length} checks passed`));
  }
}

main();
