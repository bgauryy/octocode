/**
 * Response-state hints for lspCallHierarchy.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/lsp_call_hierarchy/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const { currentPage, totalPages, hasMorePages } = ctx;

    if (hasMorePages) {
      out.push(`Page ${currentPage}/${totalPages}.`);
    }
    return out;
  },

  empty: (_ctx: HintContext = {}) => [],

  error: (ctx: HintContext = {}) => {
    const { depth, errorType } = ctx;

    if (errorType === 'not_a_function') {
      return ['Symbol is not a function. Use lspFindReferences instead.'];
    }
    if (errorType === 'timeout') {
      return [`Depth=${depth} caused timeout. Reduce depth or scope.`];
    }
    return [];
  },
};
