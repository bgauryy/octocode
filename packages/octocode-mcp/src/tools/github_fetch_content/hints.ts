/**
 * Response-state hints for githubGetFileContent.
 *
 * Only emits hints conditional on the response (partial content cursor,
 * mutually-exclusive arg errors, size errors, not-found). No static guidance.
 *
 * @module tools/github_fetch_content/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    if (c.isPartial && typeof c.endLine === 'number') {
      return [
        `Partial content ends at line ${c.endLine}. Use startLine=${
          (c.endLine as number) + 1
        } to continue.`,
      ];
    }
    return [];
  },

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (path) {
      return [
        `No content for '${path}'. Verify the path with githubViewRepoStructure or list parent dir.`,
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const size = typeof c.fileSize === 'number' ? `${c.fileSize}KB ` : '';
      return [
        `File ${size}exceeds 300KB cap. Use matchString or startLine/endLine.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const where = typeof c.path === 'string' ? `'${c.path}'` : 'this path';
      const branch =
        typeof c.branch === 'string' ? ` on branch '${c.branch}'` : '';
      return [
        `${where} not found${branch}. Confirm with githubViewRepoStructure.`,
      ];
    }
    return [];
  },
};
