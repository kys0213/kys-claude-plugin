#!/usr/bin/env bun
/**
 * tc - Team Claude CLI
 *
 * Usage:
 *   tc setup                 환경 초기화 (Flow, PSM, HUD 포함)
 *   tc setup status          Setup 상태 확인
 *   tc setup init            전체 초기화
 *   tc config info           프로젝트 정보 출력
 *   tc config verify         환경 검증
 *   tc config show           설정 파일 출력
 *   tc config get <path>     설정 값 읽기
 *   tc config set <path> <value>  설정 값 쓰기
 *   tc test unit             유닛 테스트
 *   tc test integration      통합 테스트
 *   tc test all              모든 테스트
 *   tc flow start            워크플로우 시작
 *   tc flow status           워크플로우 상태
 *   tc psm new               PSM 세션 생성
 *   tc psm list              PSM 세션 목록
 *   tc hud output            HUD 출력 (statusline용)
 */

import { Command } from "commander";
import { createSetupCommand } from "./commands/setup";
import { createConfigCommand } from "./commands/config";
import { createTestCommand } from "./commands/test";
import { createFlowCommand } from "./commands/flow";
import { createPsmCommand } from "./commands/psm";
import { createHudCommand } from "./commands/hud";

const program = new Command();

program
  .name("tc")
  .description("Team Claude CLI - 프로젝트 관리 및 테스트 도구")
  .version("0.5.0");

// 명령어 등록
program.addCommand(createSetupCommand());
program.addCommand(createConfigCommand());
program.addCommand(createTestCommand());
program.addCommand(createFlowCommand());
program.addCommand(createPsmCommand());
program.addCommand(createHudCommand());

// 기본 동작: 도움말
program.action(() => {
  program.outputHelp();
});

// 실행
program.parseAsync(process.argv).catch((err) => {
  console.error(err);
  process.exit(1);
});
