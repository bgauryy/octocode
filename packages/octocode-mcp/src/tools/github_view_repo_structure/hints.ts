/**
 * Response-state hints for githubViewRepoStructure.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/github_view_repo_structure/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    if (ctx.entryCount && ctx.entryCount > 50) {
      return [
        `Large directory (${ctx.entryCount} entries). Narrow with path or depth=1.`,
      ];
    }
    return [];
  },

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' && c.path ? c.path : undefined;
    const depth = typeof c.depth === 'number' ? c.depth : undefined;
    const branch =
      typeof c.branch === 'string' && c.branch ? c.branch : undefined;
    const where = path ? `'${path}'` : 'the repository root';
    const onBranch = branch ? ` on branch '${branch}'` : '';
    if (depth === 1) {
      return [
        `Empty listing for ${where}${onBranch}. Try depth=2 or a different path.`,
      ];
    }
    return [
      `Empty listing for ${where}${onBranch}. Verify the path with a parent directory or different branch.`,
    ];
  },

  error: (_ctx: HintContext = {}) => [],
};
