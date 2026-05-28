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
  empty: () => [],

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const size = typeof c.fileSize === 'number' ? `${c.fileSize}KB ` : '';
      return [`File ${size}exceeds the 300KB cap.`];
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const where = typeof c.path === 'string' ? `'${c.path}'` : 'path';
      const branch =
        typeof c.branch === 'string' ? ` on branch '${c.branch}'` : '';
      return [`${where} not found${branch}.`];
    }
    return [];
  },
};
