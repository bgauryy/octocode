/**
 * Response-state hints for githubViewRepoStructure.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/github_view_repo_structure/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' && c.path ? c.path : undefined;
    const branch =
      typeof c.branch === 'string' && c.branch ? c.branch : undefined;
    if (!path && !branch) return [];
    const where = path ? `'${path}'` : 'root';
    const onBranch = branch ? ` on branch '${branch}'` : '';
    return [`Empty listing for ${where}${onBranch}.`];
  },

  error: () => [],
};
