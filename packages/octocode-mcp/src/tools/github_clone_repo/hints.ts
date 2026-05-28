/**
 * Dynamic hints for githubCloneRepo tool
 * @module tools/github_clone_repo/hints
 *
 * Note: Primary hints (clone type, cache, sparse) are handled inline in
 * execution.ts via extraHints. This module provides supplementary
 * context-aware hints through the standard ToolHintGenerators interface.
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: () => [],

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'permission') return ['Token lacks read access.'];
    if (ctx.errorType === 'not_found') {
      return ['Repo or branch not found (may be private or deleted).'];
    }
    if (ctx.errorType === 'timeout') return ['Clone timed out.'];
    return [];
  },
};
