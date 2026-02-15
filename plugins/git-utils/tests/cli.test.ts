import { describe, test } from 'bun:test';

// ============================================================
// CLI Entry Point — Black-box Test Spec
// ============================================================
// parseArgs 유틸리티 + top-level 라우팅 테스트
// ============================================================

describe('parseArgs', () => {
  test.todo('positional 인자만 파싱한다');
  // input:  ['feat', 'add feature']
  // expect: { positional: ['feat', 'add feature'], flags: {} }

  test.todo('--key=value 형태 플래그를 파싱한다');
  // input:  ['feat', '--scope=auth', '--body=hello world']
  // expect: { positional: ['feat'], flags: { scope: 'auth', body: 'hello world' } }

  test.todo('--flag (값 없음) 형태를 boolean true로 파싱한다');
  // input:  ['feat', 'desc', '--skip-add']
  // expect: { flags: { 'skip-add': true } }

  test.todo('positional과 flag가 섞여도 올바르게 분리한다');
  // input:  ['feat', '--scope=auth', 'description', '--skip-add']
  // expect: { positional: ['feat', 'description'], flags: { scope: 'auth', 'skip-add': true } }

  test.todo('인자 없으면 빈 결과를 반환한다');
  // input:  []
  // expect: { positional: [], flags: {} }

  test.todo('--key= (빈 값)을 빈 문자열로 처리한다');
  // input:  ['--scope=']
  // expect: { flags: { scope: '' } }

  test.todo('--key=a=b 형태에서 첫 번째 = 기준으로 분리한다');
  // input:  ['--project-dir=/home/user/my=project']
  // expect: { flags: { 'project-dir': '/home/user/my=project' } }
});

describe('CLI routing', () => {
  test.todo('인자 없이 실행하면 help를 출력하고 exit 0');
  test.todo('--help 플래그로 help를 출력하고 exit 0');
  test.todo('--version 플래그로 버전을 출력하고 exit 0');
  test.todo('-v 플래그로 버전을 출력하고 exit 0');
  test.todo('알 수 없는 command면 에러 메시지 출력 후 exit 1');
  test.todo('유효한 command면 해당 핸들러로 dispatch');
});
