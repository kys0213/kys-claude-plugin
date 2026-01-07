import { readFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { glob } from 'glob';

const SEMVER_REGEX = /^\d+\.\d+\.\d+(-[\w.]+)?$/;

/**
 * 모든 버전 검증 실행
 */
export async function validateVersions(repoRoot) {
  const results = {
    passed: [],
    failed: [],
  };

  const marketplaceFile = `${repoRoot}/.claude-plugin/marketplace.json`;

  // 1. 개별 plugin.json 버전 형식 검증
  const pluginJsonFiles = await glob('**/plugin.json', { cwd: repoRoot, absolute: true });
  for (const file of pluginJsonFiles) {
    const result = validateVersionFormat(file);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 2. marketplace.json 버전 형식 검증
  if (existsSync(marketplaceFile)) {
    const result = validateMarketplaceVersions(marketplaceFile);
    (result.valid ? results.passed : results.failed).push(result);
  }

  // 3. marketplace.json ↔ plugin.json 버전 일관성 검증
  if (existsSync(marketplaceFile)) {
    const consistencyResults = await validateVersionConsistency(marketplaceFile, repoRoot);
    results.passed.push(...consistencyResults.filter(r => r.valid));
    results.failed.push(...consistencyResults.filter(r => !r.valid));
  }

  return results;
}

/**
 * 버전 형식 검증 (Semantic Versioning)
 */
function validateVersionFormat(filePath) {
  const errors = [];

  try {
    const content = readFileSync(filePath, 'utf-8');
    const json = JSON.parse(content);

    if (json.version) {
      if (!SEMVER_REGEX.test(json.version)) {
        errors.push(`Invalid version format: '${json.version}' (expected: MAJOR.MINOR.PATCH)`);
      }
    }

  } catch (e) {
    errors.push(`Parse error: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'version-format',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * marketplace.json 내 모든 버전 형식 검증
 */
function validateMarketplaceVersions(filePath) {
  const errors = [];

  try {
    const content = readFileSync(filePath, 'utf-8');
    const json = JSON.parse(content);

    // metadata.version 검증
    if (json.metadata?.version) {
      if (!SEMVER_REGEX.test(json.metadata.version)) {
        errors.push(`Invalid metadata.version: '${json.metadata.version}'`);
      }
    }

    // 각 플러그인 버전 검증
    if (json.plugins && Array.isArray(json.plugins)) {
      json.plugins.forEach((plugin, index) => {
        if (plugin.version && !SEMVER_REGEX.test(plugin.version)) {
          errors.push(`Invalid plugins[${index}].version: '${plugin.version}'`);
        }
      });
    }

  } catch (e) {
    errors.push(`Parse error: ${e.message}`);
  }

  return {
    file: filePath,
    type: 'marketplace-versions',
    valid: errors.length === 0,
    errors,
  };
}

/**
 * marketplace.json ↔ plugin.json 버전 일관성 검증
 */
async function validateVersionConsistency(marketplacePath, repoRoot) {
  const results = [];

  try {
    const content = readFileSync(marketplacePath, 'utf-8');
    const marketplace = JSON.parse(content);

    if (!marketplace.plugins || !Array.isArray(marketplace.plugins)) {
      return results;
    }

    for (const plugin of marketplace.plugins) {
      const source = typeof plugin.source === 'string' ? plugin.source : plugin.source?.path;
      if (!source) continue;

      const pluginPath = resolve(repoRoot, source.replace(/^\.\//, ''));
      const pluginJsonPath = resolve(pluginPath, '.claude-plugin', 'plugin.json');

      if (!existsSync(pluginJsonPath)) continue;

      try {
        const pluginContent = readFileSync(pluginJsonPath, 'utf-8');
        const pluginJson = JSON.parse(pluginContent);

        const marketplaceVersion = plugin.version;
        const pluginVersion = pluginJson.version;

        // 둘 다 버전이 있을 경우에만 일관성 검사
        if (marketplaceVersion && pluginVersion) {
          if (marketplaceVersion !== pluginVersion) {
            results.push({
              file: pluginJsonPath,
              type: 'version-consistency',
              plugin: plugin.name,
              valid: false,
              error: `Version mismatch: marketplace.json has '${marketplaceVersion}', plugin.json has '${pluginVersion}'`,
            });
          } else {
            results.push({
              file: pluginJsonPath,
              type: 'version-consistency',
              plugin: plugin.name,
              valid: true,
              error: null,
            });
          }
        }

      } catch (e) {
        results.push({
          file: pluginJsonPath,
          type: 'version-consistency',
          plugin: plugin.name,
          valid: false,
          error: `Parse error: ${e.message}`,
        });
      }
    }

  } catch (e) {
    results.push({
      file: marketplacePath,
      type: 'version-consistency',
      valid: false,
      error: `Parse error: ${e.message}`,
    });
  }

  return results;
}

/**
 * 버전 bump 계산
 */
export function bumpVersion(currentVersion, bumpType) {
  const match = currentVersion.match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) {
    throw new Error(`Invalid version: ${currentVersion}`);
  }

  let [, major, minor, patch] = match.map(Number);

  switch (bumpType) {
    case 'major':
      return `${major + 1}.0.0`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    default:
      throw new Error(`Invalid bump type: ${bumpType}`);
  }
}

/**
 * PR 타이틀에서 bump 타입 결정
 */
export function getBumpTypeFromPRTitle(title) {
  if (/^major(\(.+\))?:/.test(title)) return 'major';
  if (/^feat(\(.+\))?:/.test(title)) return 'minor';
  if (/^fix(\(.+\))?:/.test(title)) return 'patch';
  return null;
}
