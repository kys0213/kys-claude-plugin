// ============================================================
// Shell — subprocess 실행 유틸리티
// ============================================================

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
}

/**
 * 명령어를 실행하고 결과를 반환합니다.
 * 실패 시 throw하지 않고 exitCode를 반환합니다.
 */
export async function exec(
  command: string[],
  options?: { cwd?: string },
): Promise<ExecResult> {
  const proc = Bun.spawn(command, {
    stdout: 'pipe',
    stderr: 'pipe',
    cwd: options?.cwd,
  });

  const [stdout, stderr] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
  ]);

  const exitCode = await proc.exited;

  return {
    stdout: stdout.trimEnd(),
    stderr: stderr.trimEnd(),
    exitCode,
  };
}

/**
 * 명령어를 실행하고, 실패 시 에러를 throw합니다.
 */
export async function execOrThrow(
  command: string[],
  options?: { cwd?: string },
): Promise<string> {
  const result = await exec(command, options);
  if (result.exitCode !== 0) {
    throw new Error(`Command failed (exit ${result.exitCode}): ${command.join(' ')}\n${result.stderr}`);
  }
  return result.stdout;
}
