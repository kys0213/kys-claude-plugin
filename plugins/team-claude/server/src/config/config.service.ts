import { readFile, writeFile, mkdir } from "fs/promises";
import { existsSync } from "fs";
import { join, dirname } from "path";
import { homedir } from "os";
import type {
  TeamClaudeConfig,
  ConfigPath,
  WorkerTemplate,
  ReviewRule,
} from "./config.types";
import { DEFAULT_CONFIG, CONFIG_VALIDATION } from "./config.defaults";

const GLOBAL_CONFIG_DIR = join(homedir(), ".team-claude");
const GLOBAL_CONFIG_PATH = join(GLOBAL_CONFIG_DIR, "config.json");
const PROJECT_CONFIG_DIR = ".team-claude";
const PROJECT_CONFIG_FILE = "config.json";

/**
 * Configuration Service
 * 설정 로드, 저장, 병합, 유효성 검사를 담당
 */
export class ConfigService {
  private globalConfig: Partial<TeamClaudeConfig> = {};
  private projectConfig: Partial<TeamClaudeConfig> = {};
  private sessionConfig: Partial<TeamClaudeConfig> = {};
  private mergedConfig: TeamClaudeConfig = { ...DEFAULT_CONFIG };
  private projectRoot: string;

  constructor(projectRoot?: string) {
    this.projectRoot = projectRoot || process.env.PROJECT_ROOT || process.cwd();
  }

  /**
   * 모든 설정 레이어 로드 및 병합
   */
  async load(): Promise<TeamClaudeConfig> {
    // Global 설정 로드
    this.globalConfig = await this.loadConfigFile(GLOBAL_CONFIG_PATH);

    // Project 설정 로드
    const projectConfigPath = join(
      this.projectRoot,
      PROJECT_CONFIG_DIR,
      PROJECT_CONFIG_FILE
    );
    this.projectConfig = await this.loadConfigFile(projectConfigPath);

    // 병합: defaults < global < project < session
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );

    return this.mergedConfig;
  }

  /**
   * 현재 병합된 설정 반환
   */
  getConfig(): TeamClaudeConfig {
    return this.mergedConfig;
  }

  /**
   * 특정 설정값 조회 (점 표기법 지원)
   */
  get<T = unknown>(path: ConfigPath): T | undefined {
    return this.getValueByPath(this.mergedConfig, path) as T | undefined;
  }

  /**
   * 설정값 변경
   */
  async set(
    path: ConfigPath,
    value: unknown,
    scope: "global" | "project" | "session" = "project"
  ): Promise<{ success: boolean; error?: string }> {
    // 유효성 검사
    const validation = this.validateValue(path, value);
    if (!validation.valid) {
      return { success: false, error: validation.error };
    }

    // 스코프에 따라 저장
    switch (scope) {
      case "global":
        this.setValueByPath(this.globalConfig, path, value);
        await this.saveGlobalConfig();
        break;
      case "project":
        this.setValueByPath(this.projectConfig, path, value);
        await this.saveProjectConfig();
        break;
      case "session":
        this.setValueByPath(this.sessionConfig, path, value);
        break;
    }

    // 병합된 설정 갱신
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );

    return { success: true };
  }

  /**
   * 설정 섹션 또는 전체 초기화
   */
  async reset(
    section?: keyof TeamClaudeConfig,
    scope: "global" | "project" | "session" = "project"
  ): Promise<void> {
    const targetConfig =
      scope === "global"
        ? this.globalConfig
        : scope === "project"
          ? this.projectConfig
          : this.sessionConfig;

    if (section) {
      delete (targetConfig as Record<string, unknown>)[section];
    } else {
      // 전체 초기화
      Object.keys(targetConfig).forEach((key) => {
        delete (targetConfig as Record<string, unknown>)[key];
      });
    }

    if (scope === "global") {
      await this.saveGlobalConfig();
    } else if (scope === "project") {
      await this.saveProjectConfig();
    }

    // 병합된 설정 갱신
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );
  }

  /**
   * 설정 목록 조회 (플랫한 형태)
   */
  list(): Array<{
    path: string;
    value: unknown;
    default: unknown;
    source: "default" | "global" | "project" | "session";
    description?: string;
  }> {
    const result: Array<{
      path: string;
      value: unknown;
      default: unknown;
      source: "default" | "global" | "project" | "session";
      description?: string;
    }> = [];

    const flatten = (obj: Record<string, unknown>, prefix = ""): void => {
      for (const [key, value] of Object.entries(obj)) {
        const path = prefix ? `${prefix}.${key}` : key;

        if (
          typeof value === "object" &&
          value !== null &&
          !Array.isArray(value) &&
          key !== "templates"
        ) {
          flatten(value as Record<string, unknown>, path);
        } else if (key !== "templates" && key !== "rules") {
          const defaultValue = this.getValueByPath(DEFAULT_CONFIG, path);
          const globalValue = this.getValueByPath(this.globalConfig, path);
          const projectValue = this.getValueByPath(this.projectConfig, path);
          const sessionValue = this.getValueByPath(this.sessionConfig, path);

          let source: "default" | "global" | "project" | "session" = "default";
          if (sessionValue !== undefined) source = "session";
          else if (projectValue !== undefined) source = "project";
          else if (globalValue !== undefined) source = "global";

          result.push({
            path,
            value,
            default: defaultValue,
            source,
            description: CONFIG_VALIDATION[path]?.description,
          });
        }
      }
    };

    flatten(this.mergedConfig as unknown as Record<string, unknown>);
    return result;
  }

  /**
   * 설정 내보내기
   */
  async export(
    options: {
      includeTemplates?: boolean;
      includeRules?: boolean;
      includeSensitive?: boolean;
    } = {}
  ): Promise<TeamClaudeConfig> {
    const config = { ...this.mergedConfig };

    if (!options.includeTemplates) {
      // 내장 템플릿만 제외, 사용자 템플릿은 포함
      const builtInNames = ["minimal", "standard", "strict"];
      config.templates = Object.fromEntries(
        Object.entries(config.templates).filter(
          ([name]) => !builtInNames.includes(name)
        )
      );
    }

    if (!options.includeRules) {
      config.review = { ...config.review, rules: [] };
    }

    if (!options.includeSensitive) {
      // 민감 정보 제거
      if (config.notification.slack) {
        config.notification.slack = {
          ...config.notification.slack,
          webhookUrl: "***",
        };
      }
      if (config.notification.webhook) {
        config.notification.webhook = {
          ...config.notification.webhook,
          url: "***",
        };
      }
    }

    return config;
  }

  /**
   * 설정 가져오기
   */
  async import(
    config: Partial<TeamClaudeConfig>,
    scope: "global" | "project" = "project"
  ): Promise<{ success: boolean; changes: string[] }> {
    const changes: string[] = [];

    const applyChanges = (
      source: Record<string, unknown>,
      prefix = ""
    ): void => {
      for (const [key, value] of Object.entries(source)) {
        const path = prefix ? `${prefix}.${key}` : key;

        if (
          typeof value === "object" &&
          value !== null &&
          !Array.isArray(value) &&
          key !== "templates" &&
          key !== "rules"
        ) {
          applyChanges(value as Record<string, unknown>, path);
        } else {
          const currentValue = this.get(path);
          if (JSON.stringify(currentValue) !== JSON.stringify(value)) {
            changes.push(`${path}: ${JSON.stringify(currentValue)} → ${JSON.stringify(value)}`);
          }
        }
      }
    };

    applyChanges(config as Record<string, unknown>);

    // 설정 적용
    if (scope === "global") {
      this.globalConfig = this.mergeConfigs(
        this.globalConfig,
        config
      ) as Partial<TeamClaudeConfig>;
      await this.saveGlobalConfig();
    } else {
      this.projectConfig = this.mergeConfigs(
        this.projectConfig,
        config
      ) as Partial<TeamClaudeConfig>;
      await this.saveProjectConfig();
    }

    // 병합된 설정 갱신
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );

    return { success: true, changes };
  }

  /**
   * 템플릿 추가/수정
   */
  async setTemplate(
    name: string,
    template: WorkerTemplate,
    scope: "global" | "project" = "project"
  ): Promise<void> {
    const targetConfig =
      scope === "global" ? this.globalConfig : this.projectConfig;

    if (!targetConfig.templates) {
      targetConfig.templates = {};
    }
    targetConfig.templates[name] = template;

    if (scope === "global") {
      await this.saveGlobalConfig();
    } else {
      await this.saveProjectConfig();
    }

    // 병합된 설정 갱신
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );
  }

  /**
   * 템플릿 조회
   */
  getTemplate(name: string): WorkerTemplate | undefined {
    return this.mergedConfig.templates[name];
  }

  /**
   * 템플릿 목록
   */
  listTemplates(): Array<{
    name: string;
    description: string;
    isBuiltIn: boolean;
  }> {
    const builtInNames = ["minimal", "standard", "strict"];
    return Object.entries(this.mergedConfig.templates).map(
      ([name, template]) => ({
        name,
        description: template.description,
        isBuiltIn: builtInNames.includes(name),
      })
    );
  }

  /**
   * 리뷰 규칙 추가
   */
  async addRule(
    rule: ReviewRule,
    scope: "global" | "project" = "project"
  ): Promise<void> {
    const targetConfig =
      scope === "global" ? this.globalConfig : this.projectConfig;

    if (!targetConfig.review) {
      targetConfig.review = { autoLevel: "semi-auto", requireApproval: true, rules: [] };
    }
    if (!targetConfig.review.rules) {
      targetConfig.review.rules = [];
    }

    // 기존 규칙 업데이트 또는 추가
    const existingIndex = targetConfig.review.rules.findIndex(
      (r) => r.name === rule.name
    );
    if (existingIndex >= 0) {
      targetConfig.review.rules[existingIndex] = rule;
    } else {
      targetConfig.review.rules.push(rule);
    }

    if (scope === "global") {
      await this.saveGlobalConfig();
    } else {
      await this.saveProjectConfig();
    }

    // 병합된 설정 갱신
    this.mergedConfig = this.mergeConfigs(
      DEFAULT_CONFIG,
      this.globalConfig,
      this.projectConfig,
      this.sessionConfig
    );
  }

  /**
   * 리뷰 규칙 목록
   */
  listRules(): ReviewRule[] {
    return this.mergedConfig.review.rules;
  }

  // === Private Methods ===

  private async loadConfigFile(
    path: string
  ): Promise<Partial<TeamClaudeConfig>> {
    try {
      if (!existsSync(path)) {
        return {};
      }
      const content = await readFile(path, "utf-8");
      return JSON.parse(content);
    } catch {
      return {};
    }
  }

  private async saveGlobalConfig(): Promise<void> {
    await mkdir(GLOBAL_CONFIG_DIR, { recursive: true });
    await writeFile(
      GLOBAL_CONFIG_PATH,
      JSON.stringify(this.globalConfig, null, 2),
      "utf-8"
    );
  }

  private async saveProjectConfig(): Promise<void> {
    const configDir = join(this.projectRoot, PROJECT_CONFIG_DIR);
    const configPath = join(configDir, PROJECT_CONFIG_FILE);
    await mkdir(configDir, { recursive: true });
    await writeFile(
      configPath,
      JSON.stringify(this.projectConfig, null, 2),
      "utf-8"
    );
  }

  private mergeConfigs(
    ...configs: Partial<TeamClaudeConfig>[]
  ): TeamClaudeConfig {
    const result = {} as TeamClaudeConfig;

    for (const config of configs) {
      this.deepMerge(result, config);
    }

    return result;
  }

  private deepMerge(target: Record<string, unknown>, source: Record<string, unknown>): void {
    for (const [key, value] of Object.entries(source)) {
      if (
        typeof value === "object" &&
        value !== null &&
        !Array.isArray(value) &&
        typeof target[key] === "object" &&
        target[key] !== null
      ) {
        this.deepMerge(
          target[key] as Record<string, unknown>,
          value as Record<string, unknown>
        );
      } else if (value !== undefined) {
        target[key] = value;
      }
    }
  }

  private getValueByPath(obj: unknown, path: string): unknown {
    const parts = path.split(".");
    let current = obj as Record<string, unknown>;

    for (const part of parts) {
      if (current === undefined || current === null) {
        return undefined;
      }
      current = current[part] as Record<string, unknown>;
    }

    return current;
  }

  private setValueByPath(
    obj: Record<string, unknown>,
    path: string,
    value: unknown
  ): void {
    const parts = path.split(".");
    let current = obj;

    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      if (!(part in current) || typeof current[part] !== "object") {
        current[part] = {};
      }
      current = current[part] as Record<string, unknown>;
    }

    current[parts[parts.length - 1]] = value;
  }

  private validateValue(
    path: string,
    value: unknown
  ): { valid: boolean; error?: string } {
    const rule = CONFIG_VALIDATION[path];
    if (!rule) {
      // 알려지지 않은 경로도 허용 (커스텀 설정)
      return { valid: true };
    }

    switch (rule.type) {
      case "number":
        if (typeof value !== "number") {
          return { valid: false, error: `${path}는 숫자여야 합니다.` };
        }
        if (rule.min !== undefined && value < rule.min) {
          return {
            valid: false,
            error: `${path}는 ${rule.min} 이상이어야 합니다.`,
          };
        }
        if (rule.max !== undefined && value > rule.max) {
          return {
            valid: false,
            error: `${path}는 ${rule.max} 이하여야 합니다.`,
          };
        }
        break;

      case "string":
        if (typeof value !== "string") {
          return { valid: false, error: `${path}는 문자열이어야 합니다.` };
        }
        break;

      case "boolean":
        if (typeof value !== "boolean") {
          return { valid: false, error: `${path}는 true/false여야 합니다.` };
        }
        break;

      case "enum":
        if (!rule.enum?.includes(value as string)) {
          return {
            valid: false,
            error: `${path}는 ${rule.enum?.join(", ")} 중 하나여야 합니다.`,
          };
        }
        break;
    }

    return { valid: true };
  }
}

// Singleton instance
let configServiceInstance: ConfigService | null = null;

export function getConfigService(projectRoot?: string): ConfigService {
  if (!configServiceInstance) {
    configServiceInstance = new ConfigService(projectRoot);
  }
  return configServiceInstance;
}

export function resetConfigService(): void {
  configServiceInstance = null;
}
