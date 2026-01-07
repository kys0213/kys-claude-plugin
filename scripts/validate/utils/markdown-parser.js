import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkFrontmatter from 'remark-frontmatter';
import { parse as parseYaml } from 'yaml';
import { readFileSync } from 'fs';

/**
 * 마크다운 파일을 AST로 파싱
 */
export function parseMarkdown(filePath) {
  const content = readFileSync(filePath, 'utf-8');

  const processor = unified()
    .use(remarkParse)
    .use(remarkFrontmatter, ['yaml']);

  const tree = processor.parse(content);

  return {
    content,
    tree,
    frontmatter: extractFrontmatter(tree),
  };
}

/**
 * AST에서 YAML frontmatter 추출
 */
function extractFrontmatter(tree) {
  const yamlNode = tree.children.find(node => node.type === 'yaml');

  if (!yamlNode) {
    return null;
  }

  try {
    return parseYaml(yamlNode.value);
  } catch (error) {
    return { _parseError: error.message };
  }
}

/**
 * AST에서 인라인 코드 내 경로 추출 (코드 블록 제외)
 */
export function extractInlineCodePaths(tree) {
  const paths = [];

  walkTree(tree, (node, parent) => {
    // 코드 블록 내부는 제외
    if (parent?.type === 'code') return;

    if (node.type === 'inlineCode') {
      const value = node.value;
      // 파일 경로 패턴 (확장자가 있는 것)
      if (/\.(md|sh|py|js|ts|json|yaml|yml)$/.test(value)) {
        paths.push({
          type: 'inlineCode',
          value,
          position: node.position,
        });
      }
      // ${CLAUDE_PLUGIN_ROOT} 경로
      if (value.includes('${CLAUDE_PLUGIN_ROOT}')) {
        paths.push({
          type: 'pluginRootPath',
          value,
          position: node.position,
        });
      }
    }
  });

  return paths;
}

/**
 * AST에서 마크다운 링크 추출
 */
export function extractLinks(tree) {
  const links = [];

  walkTree(tree, (node) => {
    if (node.type === 'link') {
      links.push({
        type: 'link',
        url: node.url,
        position: node.position,
      });
    }
  });

  return links;
}

/**
 * AST에서 코드 블록 내 경로 참조 추출 (bash 스크립트 등)
 */
export function extractCodeBlockPaths(tree) {
  const paths = [];

  walkTree(tree, (node) => {
    if (node.type === 'code' && node.value) {
      // ${CLAUDE_PLUGIN_ROOT} 패턴 추출
      const pluginRootMatches = node.value.match(/\$\{CLAUDE_PLUGIN_ROOT\}[^\s"'`]+/g);
      if (pluginRootMatches) {
        pluginRootMatches.forEach(match => {
          paths.push({
            type: 'codeBlockPath',
            value: match,
            lang: node.lang,
            position: node.position,
          });
        });
      }
    }
  });

  return paths;
}

/**
 * 모든 경로 참조 추출 (통합)
 */
export function extractAllPaths(tree) {
  return {
    inlineCode: extractInlineCodePaths(tree),
    links: extractLinks(tree),
    codeBlocks: extractCodeBlockPaths(tree),
  };
}

/**
 * AST 트리 순회 헬퍼
 */
function walkTree(node, callback, parent = null) {
  callback(node, parent);

  if (node.children) {
    for (const child of node.children) {
      walkTree(child, callback, node);
    }
  }
}

/**
 * Frontmatter 필수 필드 검증
 */
export function validateFrontmatter(frontmatter, requiredFields, filePath) {
  const errors = [];

  if (!frontmatter) {
    errors.push({
      file: filePath,
      error: 'Missing YAML frontmatter',
    });
    return errors;
  }

  if (frontmatter._parseError) {
    errors.push({
      file: filePath,
      error: `Invalid YAML: ${frontmatter._parseError}`,
    });
    return errors;
  }

  for (const field of requiredFields) {
    if (!frontmatter[field]) {
      errors.push({
        file: filePath,
        field,
        error: `Missing required field: ${field}`,
      });
    }
  }

  return errors;
}
