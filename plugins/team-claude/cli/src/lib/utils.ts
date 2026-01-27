/**
 * 유틸리티 함수
 */

import chalk from "chalk";

// ============================================================================
// 출력 헬퍼
// ============================================================================

export const log = {
  info: (msg: string) => console.log(chalk.blue("[INFO]"), msg),
  ok: (msg: string) => console.log(chalk.green("[OK]"), msg),
  warn: (msg: string) => console.log(chalk.yellow("[WARN]"), msg),
  err: (msg: string) => console.error(chalk.red("[ERR]"), msg),
};

export const icon = {
  check: chalk.green("✓"),
  cross: chalk.red("✗"),
  warn: chalk.yellow("⚠"),
  dot: chalk.blue("●"),
};

export function printSection(title: string): void {
  console.log();
  console.log(chalk.bold(`━━━ ${title} ━━━`));
  console.log();
}

export function printKV(key: string, value: string, indent = 2): void {
  const spaces = " ".repeat(indent);
  console.log(`${spaces}${chalk.gray(key)}: ${value}`);
}

export function printStatus(
  label: string,
  ok: boolean,
  message?: string
): void {
  const status = ok ? icon.check : icon.cross;
  const text = message ? `${label} ${chalk.gray(`(${message})`)}` : label;
  console.log(`  ${status} ${text}`);
}

export function printWarning(label: string, message?: string): void {
  const text = message ? `${label} ${chalk.gray(`(${message})`)}` : label;
  console.log(`  ${icon.warn} ${text}`);
}

// ============================================================================
// 테스트 결과 포맷팅
// ============================================================================

export interface TestResult {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
}

export function printTestResults(results: TestResult[]): void {
  const passed = results.filter((r) => r.passed).length;
  const failed = results.filter((r) => !r.passed).length;
  const total = results.length;

  printSection("테스트 결과");

  for (const result of results) {
    const status = result.passed ? icon.check : icon.cross;
    const duration = chalk.gray(`(${result.duration}ms)`);
    console.log(`  ${status} ${result.name} ${duration}`);
    if (result.error) {
      console.log(chalk.red(`      ${result.error}`));
    }
  }

  console.log();
  console.log("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  if (failed === 0) {
    console.log(chalk.green(`✓ 모든 테스트 통과 (${passed}/${total})`));
  } else {
    console.log(chalk.red(`✗ 실패 ${failed}개, 성공 ${passed}개 (총 ${total}개)`));
  }
  console.log();
}

// ============================================================================
// JSON 출력
// ============================================================================

export function printJson(data: unknown): void {
  console.log(JSON.stringify(data, null, 2));
}

// ============================================================================
// 시간 측정
// ============================================================================

export function measure<T>(fn: () => T): { result: T; duration: number } {
  const start = performance.now();
  const result = fn();
  const duration = Math.round(performance.now() - start);
  return { result, duration };
}

export async function measureAsync<T>(
  fn: () => Promise<T>
): Promise<{ result: T; duration: number }> {
  const start = performance.now();
  const result = await fn();
  const duration = Math.round(performance.now() - start);
  return { result, duration };
}
