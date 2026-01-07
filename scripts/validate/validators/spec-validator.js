import { readFileSync, existsSync } from 'fs';
import { glob } from 'glob';
import { parseMarkdown, validateFrontmatter } from '../utils/markdown-parser.js';

/**
 * 모든 스펙 검증 실행
 */
export async function validateSpecs(repoRoot) {
  const results = {
    passed: [],
    failed: [],
  };

  // 1. plugin.json 검증
  const pluginJsonFiles = await glob('**/plugin.json', { cwd: repoRoot, absolute: true });
  for (const file of pluginJsonFiles) {
    const result = validatePluginJson(file);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 2. marketplace.json 검증
  const marketplaceFile = `${repoRoot}/.claude-plugin/marketplace.json`;
  if (existsSync(marketplaceFile)) {
    const result = validateMarketplaceJson(marketplaceFile);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 3. SKILL.md 검증
  const skillFiles = await glob('**/skills/*/SKILL.md', { cwd: repoRoot, absolute: true });
  for (const file of skillFiles) {
    const result = validateSkillMd(file);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 4. Agent 검증
  const agentFiles = await glob('**/agents/*.md', { cwd: repoRoot, absolute: true });
  for (const file of agentFiles) {
    const result = validateAgentMd(file);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 5. Command 검증
  const commandFiles = await glob('**/commands/*.md', { cwd: repoRoot, absolute: true });
  for (const file of commandFiles) {
    const result = validateCommandMd(file);
    (result.valid ? results.passed : results.failed).push(result);
  }

  return results;
}

/**
 * plugin.json 검증
 */
function validatePluginJson(filePath) {
  const errors = [];

  try {
    const content = readFileSync(filePath, 'utf-8');
    const json = JSON.parse(content);

    // 필수 필드
    if (!json.name) {
      errors.push('Missing required field: name');
    }

    // name 형식 (kebab-case)
    if (json.name && !/^[a-z0-9]+(-[a-z0-9]+)*$/.test(json.name)) {
      errors.push(`Invalid name format: '${json.name}' (must be kebab-case)`);
    }

    // version 형식 (있을 경우)
    if (json.version && !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(json.version)) {
      errors.push(`Invalid version format: '${json.version}' (must be semver)`);
    }

  } catch (e) {
    errors.push(`Invalid JSON: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'plugin.json',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * marketplace.json 검증
 */
function validateMarketplaceJson(filePath) {
  const errors = [];

  try {
    const content = readFileSync(filePath, 'utf-8');
    const json = JSON.parse(content);

    // 필수 필드
    if (!json.name) {
      errors.push('Missing required field: name');
    }

    if (!json.owner?.name) {
      errors.push('Missing required field: owner.name');
    }

    if (!json.plugins || !Array.isArray(json.plugins)) {
      errors.push('Missing or invalid field: plugins (must be array)');
    } else {
      // 각 플러그인 엔트리 검증
      json.plugins.forEach((plugin, index) => {
        if (!plugin.name) {
          errors.push(`plugins[${index}]: Missing required field 'name'`);
        }
        if (!plugin.source) {
          errors.push(`plugins[${index}]: Missing required field 'source'`);
        }
        if (plugin.version && !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(plugin.version)) {
          errors.push(`plugins[${index}]: Invalid version format '${plugin.version}'`);
        }
      });
    }

    // metadata.version 검증
    if (json.metadata?.version && !/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(json.metadata.version)) {
      errors.push(`Invalid metadata.version format: '${json.metadata.version}'`);
    }

  } catch (e) {
    errors.push(`Invalid JSON: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'marketplace.json',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * SKILL.md 검증
 */
function validateSkillMd(filePath) {
  const errors = [];

  try {
    const { frontmatter } = parseMarkdown(filePath);
    const frontmatterErrors = validateFrontmatter(
      frontmatter,
      ['name', 'description'],
      filePath
    );
    errors.push(...frontmatterErrors.map(e => e.error));

  } catch (e) {
    errors.push(`Parse error: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'skill',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Agent 검증
 */
function validateAgentMd(filePath) {
  const errors = [];

  try {
    const { frontmatter } = parseMarkdown(filePath);
    const frontmatterErrors = validateFrontmatter(
      frontmatter,
      ['name', 'description'],
      filePath
    );
    errors.push(...frontmatterErrors.map(e => e.error));

    // tools 배열 검증
    if (frontmatter?.tools && !Array.isArray(frontmatter.tools)) {
      errors.push("'tools' must be an array");
    }

    // model 값 검증
    const validModels = ['inherit', 'sonnet', 'opus', 'haiku'];
    if (frontmatter?.model && !validModels.includes(frontmatter.model)) {
      errors.push(`Invalid model: '${frontmatter.model}' (valid: ${validModels.join(', ')})`);
    }

  } catch (e) {
    errors.push(`Parse error: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'agent',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * Command 검증
 */
function validateCommandMd(filePath) {
  const errors = [];

  try {
    const { frontmatter } = parseMarkdown(filePath);
    const frontmatterErrors = validateFrontmatter(
      frontmatter,
      ['name', 'description'],
      filePath
    );
    errors.push(...frontmatterErrors.map(e => e.error));

    // allowed-tools 배열 검증
    if (frontmatter?.['allowed-tools'] && !Array.isArray(frontmatter['allowed-tools'])) {
      errors.push("'allowed-tools' must be an array");
    }

  } catch (e) {
    errors.push(`Parse error: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'command',
    valid: errors.length === 0,
    errors,
  };
}
