import { existsSync, readFileSync } from 'fs';
import { dirname, resolve, normalize } from 'path';
import { glob } from 'glob';
import { parseMarkdown, extractAllPaths } from '../utils/markdown-parser.js';

/**
 * 모든 경로 검증 실행
 */
export async function validatePaths(repoRoot) {
  const results = {
    passed: [],
    failed: [],
  };

  // 1. Skill 참조 경로 검증
  const skillFiles = await glob('**/skills/*/SKILL.md', { cwd: repoRoot, absolute: true });
  for (const file of skillFiles) {
    const pathResults = validateSkillPaths(file);
    results.passed.push(...pathResults.filter(r => r.valid));
    results.failed.push(...pathResults.filter(r => !r.valid));
  }

  // 2. Agent 참조 경로 검증
  const agentFiles = await glob('**/agents/*.md', { cwd: repoRoot, absolute: true });
  for (const file of agentFiles) {
    const pathResults = validateDocumentPaths(file, repoRoot);
    results.passed.push(...pathResults.filter(r => r.valid));
    results.failed.push(...pathResults.filter(r => !r.valid));
  }

  // 3. Command 참조 경로 검증
  const commandFiles = await glob('**/commands/*.md', { cwd: repoRoot, absolute: true });
  for (const file of commandFiles) {
    const pathResults = validateDocumentPaths(file, repoRoot);
    results.passed.push(...pathResults.filter(r => r.valid));
    results.failed.push(...pathResults.filter(r => !r.valid));
  }

  // 4. marketplace.json source 경로 검증
  const marketplaceFile = `${repoRoot}/.claude-plugin/marketplace.json`;
  if (existsSync(marketplaceFile)) {
    const sourceResults = validateMarketplaceSources(marketplaceFile, repoRoot);
    results.passed.push(...sourceResults.filter(r => r.valid));
    results.failed.push(...sourceResults.filter(r => !r.valid));
  }

  return results;
}

/**
 * glob 패턴인지 확인 (예시 경로는 검증에서 제외)
 */
function isGlobPattern(path) {
  return /[*?]/.test(path);
}

/**
 * Skill 파일 내 참조 경로 검증
 * - references/, examples/ 디렉토리 내 파일 참조
 */
function validateSkillPaths(filePath) {
  const results = [];
  const skillDir = dirname(filePath);

  try {
    const { tree } = parseMarkdown(filePath);
    const paths = extractAllPaths(tree);

    // 인라인 코드에서 추출한 경로 검증
    for (const pathInfo of paths.inlineCode) {
      // ${CLAUDE_PLUGIN_ROOT} 경로는 별도 처리
      if (pathInfo.type === 'pluginRootPath') continue;

      const refPath = pathInfo.value;

      // glob 패턴(예시 경로)은 건너뛰기
      if (isGlobPattern(refPath)) continue;

      // 스킬 내부 참조만 검증 (references/, examples/ 등)
      if (!refPath.startsWith('references/') && !refPath.startsWith('examples/')) continue;

      // 상대 경로로 간주
      const fullPath = resolve(skillDir, refPath);

      results.push({
        file: filePath,
        type: 'skill-reference',
        referencedPath: refPath,
        resolvedPath: fullPath,
        valid: existsSync(fullPath),
        error: existsSync(fullPath) ? null : `Referenced file not found: ${refPath}`,
      });
    }

  } catch (e) {
    results.push({
      file: filePath,
      type: 'skill-reference',
      valid: false,
      error: `Parse error: ${e.message}`,
    });
  }

  return results;
}

/**
 * 문서 내 ${CLAUDE_PLUGIN_ROOT} 경로 검증
 */
function validateDocumentPaths(filePath, repoRoot) {
  const results = [];
  const fileDir = dirname(filePath);

  // 플러그인 루트 찾기 (.claude-plugin 디렉토리가 있는 곳)
  const pluginRoot = findPluginRoot(fileDir);

  try {
    const { tree } = parseMarkdown(filePath);
    const paths = extractAllPaths(tree);

    // 인라인 코드에서 ${CLAUDE_PLUGIN_ROOT} 경로 검증
    for (const pathInfo of paths.inlineCode) {
      if (pathInfo.type === 'pluginRootPath') {
        const result = validatePluginRootPath(pathInfo.value, pluginRoot, repoRoot, filePath);
        results.push(result);
      }
    }

    // 코드 블록에서 ${CLAUDE_PLUGIN_ROOT} 경로 검증
    for (const pathInfo of paths.codeBlocks) {
      const result = validatePluginRootPath(pathInfo.value, pluginRoot, repoRoot, filePath);
      results.push(result);
    }

  } catch (e) {
    results.push({
      file: filePath,
      type: 'document-path',
      valid: false,
      error: `Parse error: ${e.message}`,
    });
  }

  return results;
}

/**
 * ${CLAUDE_PLUGIN_ROOT} 경로 검증
 */
function validatePluginRootPath(pathStr, pluginRoot, repoRoot, sourceFile) {
  // ${CLAUDE_PLUGIN_ROOT}를 실제 경로로 치환
  let resolvedPath = pathStr.replace(/\$\{CLAUDE_PLUGIN_ROOT\}/g, pluginRoot);

  // 상대 경로 정규화 (../ 처리) - resolve로 절대 경로 변환
  const normalizedPath = resolve(pluginRoot, resolvedPath.replace(pluginRoot, '.'));

  // 실제 존재 여부 확인
  const exists = existsSync(normalizedPath);

  return {
    file: sourceFile,
    type: 'plugin-root-path',
    referencedPath: pathStr,
    resolvedPath: normalizedPath,
    valid: exists,
    error: exists ? null : `Script/file not found: ${pathStr} (resolved to: ${normalizedPath})`,
  };
}

/**
 * marketplace.json의 source 경로 검증
 */
function validateMarketplaceSources(marketplacePath, repoRoot) {
  const results = [];

  try {
    const content = readFileSync(marketplacePath, 'utf-8');
    const json = JSON.parse(content);

    if (!json.plugins || !Array.isArray(json.plugins)) {
      return results;
    }

    for (const plugin of json.plugins) {
      const source = typeof plugin.source === 'string' ? plugin.source : plugin.source?.path;

      if (!source) continue;

      // ./plugins/review -> 절대 경로로
      const pluginPath = resolve(repoRoot, source.replace(/^\.\//, ''));
      const pluginJson = resolve(pluginPath, '.claude-plugin', 'plugin.json');

      // 디렉토리 존재 여부
      const dirExists = existsSync(pluginPath);
      // plugin.json 존재 여부
      const jsonExists = existsSync(pluginJson);

      if (!dirExists) {
        results.push({
          file: marketplacePath,
          type: 'marketplace-source',
          referencedPath: source,
          resolvedPath: pluginPath,
          valid: false,
          error: `Plugin directory not found: ${source}`,
        });
      } else if (!jsonExists) {
        results.push({
          file: marketplacePath,
          type: 'marketplace-source',
          referencedPath: source,
          resolvedPath: pluginJson,
          valid: false,
          error: `plugin.json not found for: ${source}`,
        });
      } else {
        results.push({
          file: marketplacePath,
          type: 'marketplace-source',
          referencedPath: source,
          valid: true,
          error: null,
        });
      }
    }

  } catch (e) {
    results.push({
      file: marketplacePath,
      type: 'marketplace-source',
      valid: false,
      error: `Parse error: ${e.message}`,
    });
  }

  return results;
}

/**
 * 주어진 디렉토리에서 플러그인 루트 찾기
 */
function findPluginRoot(startDir) {
  let current = startDir;

  while (current !== '/') {
    if (existsSync(resolve(current, '.claude-plugin'))) {
      return current;
    }
    current = dirname(current);
  }

  return startDir;
}
