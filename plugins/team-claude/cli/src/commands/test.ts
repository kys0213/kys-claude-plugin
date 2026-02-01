/**
 * test 명령어 - 테스트 실행
 */

import { Command } from "commander";
import { runUnitTests } from "../test/unit";
import { runIntegrationTests } from "../test/integration";
import { printTestResults, printSection, TestResult } from "../lib/utils";

export function createTestCommand(): Command {
  const cmd = new Command("test")
    .description("테스트 실행")
    .addCommand(
      new Command("unit")
        .description("함수 단위 테스트")
        .option("--json", "JSON 형식 출력")
        .action(async (opts) => {
          printSection("Unit Tests");
          const results = await runUnitTests();

          if (opts.json) {
            console.log(JSON.stringify(results, null, 2));
          } else {
            printTestResults(results);
          }

          const failed = results.filter((r) => !r.passed).length;
          process.exit(failed > 0 ? 1 : 0);
        })
    )
    .addCommand(
      new Command("integration")
        .description("통합 테스트")
        .option("--json", "JSON 형식 출력")
        .action(async (opts) => {
          printSection("Integration Tests");
          const results = await runIntegrationTests();

          if (opts.json) {
            console.log(JSON.stringify(results, null, 2));
          } else {
            printTestResults(results);
          }

          const failed = results.filter((r) => !r.passed).length;
          process.exit(failed > 0 ? 1 : 0);
        })
    )
    .addCommand(
      new Command("all")
        .description("모든 테스트 실행")
        .option("--json", "JSON 형식 출력")
        .action(async (opts) => {
          const allResults: TestResult[] = [];

          printSection("Unit Tests");
          const unitResults = await runUnitTests();
          allResults.push(...unitResults);

          printSection("Integration Tests");
          const integrationResults = await runIntegrationTests();
          allResults.push(...integrationResults);

          if (opts.json) {
            console.log(
              JSON.stringify(
                {
                  unit: unitResults,
                  integration: integrationResults,
                  summary: {
                    total: allResults.length,
                    passed: allResults.filter((r) => r.passed).length,
                    failed: allResults.filter((r) => !r.passed).length,
                  },
                },
                null,
                2
              )
            );
          } else {
            printTestResults(allResults);
          }

          const failed = allResults.filter((r) => !r.passed).length;
          process.exit(failed > 0 ? 1 : 0);
        })
    );

  // 기본 동작: 모든 테스트 실행
  cmd.action(async () => {
    const allResults: TestResult[] = [];

    printSection("Unit Tests");
    const unitResults = await runUnitTests();
    allResults.push(...unitResults);

    printSection("Integration Tests");
    const integrationResults = await runIntegrationTests();
    allResults.push(...integrationResults);

    printTestResults(allResults);

    const failed = allResults.filter((r) => !r.passed).length;
    process.exit(failed > 0 ? 1 : 0);
  });

  return cmd;
}
