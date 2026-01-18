import { Hono } from "hono";
import type { ApiResponse } from "../types";
import { getConfigService } from "../config/config.service";
import type { TeamClaudeConfig, ReviewRule, WorkerTemplate } from "../config/config.types";

const configRouter = new Hono();

/**
 * GET /config
 * 전체 설정 조회
 */
configRouter.get("/", async (c) => {
  const configService = getConfigService();
  await configService.load();

  const response: ApiResponse<TeamClaudeConfig> = {
    success: true,
    data: configService.getConfig(),
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /config/list
 * 설정 목록 (플랫한 형태)
 */
configRouter.get("/list", async (c) => {
  const configService = getConfigService();
  await configService.load();

  const list = configService.list();

  const response: ApiResponse<typeof list> = {
    success: true,
    data: list,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /config/:path
 * 특정 설정값 조회
 */
configRouter.get("/:path{.+}", async (c) => {
  const path = c.req.param("path");
  const configService = getConfigService();
  await configService.load();

  const value = configService.get(path);

  if (value === undefined) {
    const response: ApiResponse = {
      success: false,
      error: `설정을 찾을 수 없습니다: ${path}`,
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 404);
  }

  const response: ApiResponse<{ path: string; value: unknown }> = {
    success: true,
    data: { path, value },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /config/set
 * 설정값 변경
 */
configRouter.post("/set", async (c) => {
  const { path, value, scope } = await c.req.json<{
    path: string;
    value: unknown;
    scope?: "global" | "project" | "session";
  }>();

  if (!path) {
    const response: ApiResponse = {
      success: false,
      error: "path가 필요합니다.",
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 400);
  }

  const configService = getConfigService();
  await configService.load();

  const result = await configService.set(path, value, scope || "project");

  if (!result.success) {
    const response: ApiResponse = {
      success: false,
      error: result.error,
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 400);
  }

  const response: ApiResponse<{ path: string; value: unknown; scope: string }> = {
    success: true,
    data: { path, value, scope: scope || "project" },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /config/reset
 * 설정 초기화
 */
configRouter.post("/reset", async (c) => {
  const { section, scope } = await c.req.json<{
    section?: keyof TeamClaudeConfig;
    scope?: "global" | "project" | "session";
  }>();

  const configService = getConfigService();
  await configService.load();
  await configService.reset(section, scope || "project");

  const response: ApiResponse<{ section?: string; scope: string }> = {
    success: true,
    data: { section, scope: scope || "project" },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /config/export
 * 설정 내보내기
 */
configRouter.get("/export", async (c) => {
  const includeTemplates = c.req.query("templates") === "true";
  const includeRules = c.req.query("rules") === "true";
  const includeSensitive = c.req.query("sensitive") === "true";

  const configService = getConfigService();
  await configService.load();

  const exported = await configService.export({
    includeTemplates,
    includeRules,
    includeSensitive,
  });

  const response: ApiResponse<TeamClaudeConfig> = {
    success: true,
    data: exported,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /config/import
 * 설정 가져오기
 */
configRouter.post("/import", async (c) => {
  const { config, scope } = await c.req.json<{
    config: Partial<TeamClaudeConfig>;
    scope?: "global" | "project";
  }>();

  if (!config) {
    const response: ApiResponse = {
      success: false,
      error: "config가 필요합니다.",
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 400);
  }

  const configService = getConfigService();
  await configService.load();

  const result = await configService.import(config, scope || "project");

  const response: ApiResponse<{ changes: string[] }> = {
    success: true,
    data: { changes: result.changes },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /config/templates
 * 템플릿 목록
 */
configRouter.get("/templates", async (c) => {
  const configService = getConfigService();
  await configService.load();

  const templates = configService.listTemplates();

  const response: ApiResponse<typeof templates> = {
    success: true,
    data: templates,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * GET /config/templates/:name
 * 특정 템플릿 조회
 */
configRouter.get("/templates/:name", async (c) => {
  const name = c.req.param("name");
  const configService = getConfigService();
  await configService.load();

  const template = configService.getTemplate(name);

  if (!template) {
    const response: ApiResponse = {
      success: false,
      error: `템플릿을 찾을 수 없습니다: ${name}`,
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 404);
  }

  const response: ApiResponse<WorkerTemplate> = {
    success: true,
    data: template,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /config/templates
 * 템플릿 추가/수정
 */
configRouter.post("/templates", async (c) => {
  const { template, scope } = await c.req.json<{
    template: WorkerTemplate;
    scope?: "global" | "project";
  }>();

  if (!template || !template.name) {
    const response: ApiResponse = {
      success: false,
      error: "template.name이 필요합니다.",
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 400);
  }

  const configService = getConfigService();
  await configService.load();
  await configService.setTemplate(template.name, template, scope || "project");

  const response: ApiResponse<{ name: string }> = {
    success: true,
    data: { name: template.name },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 201);
});

/**
 * GET /config/rules
 * 리뷰 규칙 목록
 */
configRouter.get("/rules", async (c) => {
  const configService = getConfigService();
  await configService.load();

  const rules = configService.listRules();

  const response: ApiResponse<ReviewRule[]> = {
    success: true,
    data: rules,
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 200);
});

/**
 * POST /config/rules
 * 리뷰 규칙 추가
 */
configRouter.post("/rules", async (c) => {
  const { rule, scope } = await c.req.json<{
    rule: ReviewRule;
    scope?: "global" | "project";
  }>();

  if (!rule || !rule.name) {
    const response: ApiResponse = {
      success: false,
      error: "rule.name이 필요합니다.",
      timestamp: new Date().toISOString(),
    };
    return c.json(response, 400);
  }

  const configService = getConfigService();
  await configService.load();
  await configService.addRule(rule, scope || "project");

  const response: ApiResponse<{ name: string }> = {
    success: true,
    data: { name: rule.name },
    timestamp: new Date().toISOString(),
  };

  return c.json(response, 201);
});

export { configRouter };
