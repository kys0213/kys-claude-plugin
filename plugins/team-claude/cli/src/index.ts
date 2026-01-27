#!/usr/bin/env bun
/**
 * tc - Team Claude CLI
 *
 * Usage:
 *   tc config info          프로젝트 정보 출력
 *   tc config verify        환경 검증
 *   tc config show          설정 파일 출력
 *   tc config get <path>    설정 값 읽기
 *   tc config set <path> <value>  설정 값 쓰기
 *   tc test unit            유닛 테스트
 *   tc test integration     통합 테스트
 *   tc test all             모든 테스트
 */

import { Command } from "commander";
import { createConfigCommand } from "./commands/config";
import { createTestCommand } from "./commands/test";

const program = new Command();

program
  .name("tc")
  .description("Team Claude CLI - 프로젝트 관리 및 테스트 도구")
  .version("0.1.0");

// 명령어 등록
program.addCommand(createConfigCommand());
program.addCommand(createTestCommand());

// 기본 동작: 도움말
program.action(() => {
  program.outputHelp();
});

// 실행
program.parseAsync(process.argv).catch((err) => {
  console.error(err);
  process.exit(1);
});
