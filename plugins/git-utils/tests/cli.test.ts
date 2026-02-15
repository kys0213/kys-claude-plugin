import { describe, test, expect } from 'bun:test';
import { parseArgs } from '../src/cli';

// ============================================================
// CLI Entry Point — Black-box Test Spec
// ============================================================

describe('parseArgs', () => {
  test('positional 인자만 파싱한다', () => {
    const result = parseArgs(['feat', 'add feature']);
    expect(result).toEqual({ positional: ['feat', 'add feature'], flags: {} });
  });

  test('--key=value 형태 플래그를 파싱한다', () => {
    const result = parseArgs(['feat', '--scope=auth', '--body=hello world']);
    expect(result).toEqual({
      positional: ['feat'],
      flags: { scope: 'auth', body: 'hello world' },
    });
  });

  test('--flag (값 없음) 형태를 boolean true로 파싱한다', () => {
    const result = parseArgs(['feat', 'desc', '--skip-add']);
    expect(result).toEqual({
      positional: ['feat', 'desc'],
      flags: { 'skip-add': true },
    });
  });

  test('positional과 flag가 섞여도 올바르게 분리한다', () => {
    const result = parseArgs(['feat', '--scope=auth', 'description', '--skip-add']);
    expect(result).toEqual({
      positional: ['feat', 'description'],
      flags: { scope: 'auth', 'skip-add': true },
    });
  });

  test('인자 없으면 빈 결과를 반환한다', () => {
    const result = parseArgs([]);
    expect(result).toEqual({ positional: [], flags: {} });
  });

  test('--key= (빈 값)을 빈 문자열로 처리한다', () => {
    const result = parseArgs(['--scope=']);
    expect(result).toEqual({ positional: [], flags: { scope: '' } });
  });

  test('--key=a=b 형태에서 첫 번째 = 기준으로 분리한다', () => {
    const result = parseArgs(['--project-dir=/home/user/my=project']);
    expect(result).toEqual({
      positional: [],
      flags: { 'project-dir': '/home/user/my=project' },
    });
  });
});

describe('CLI routing', () => {
  // CLI routing은 process.exit()을 호출하므로 subprocess로 테스트
  const CLI_PATH = new URL('../src/cli.ts', import.meta.url).pathname;

  async function runCli(args: string[]): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    const proc = Bun.spawn(['bun', 'run', CLI_PATH, ...args], {
      stdout: 'pipe',
      stderr: 'pipe',
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;
    return { stdout: stdout.trim(), stderr: stderr.trim(), exitCode };
  }

  test('인자 없이 실행하면 help를 출력하고 exit 0', async () => {
    const { stdout, exitCode } = await runCli([]);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('git-utils');
    expect(stdout).toContain('Commands:');
  });

  test('--help 플래그로 help를 출력하고 exit 0', async () => {
    const { stdout, exitCode } = await runCli(['--help']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('Commands:');
  });

  test('--version 플래그로 버전을 출력하고 exit 0', async () => {
    const { stdout, exitCode } = await runCli(['--version']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^git-utils v\d+\.\d+\.\d+/);
  });

  test('-v 플래그로 버전을 출력하고 exit 0', async () => {
    const { stdout, exitCode } = await runCli(['-v']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^git-utils v\d+\.\d+\.\d+/);
  });

  test('알 수 없는 command면 에러 메시지 출력 후 exit 1', async () => {
    const { stderr, exitCode } = await runCli(['unknown-cmd']);
    expect(exitCode).toBe(1);
    expect(stderr).toContain('Unknown command');
  });

  test('유효한 command면 해당 핸들러로 dispatch', async () => {
    const { stdout, exitCode } = await runCli(['commit', 'feat', 'test']);
    expect(exitCode).toBe(0);
    expect(stdout).toContain('command=commit');
  });
});
