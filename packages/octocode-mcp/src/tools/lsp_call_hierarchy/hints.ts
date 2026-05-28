/**
 * Response-state hints for lspCallHierarchy.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/lsp_call_hierarchy/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: () => [],

  error: (ctx: HintContext = {}) => {
    const { depth, errorType } = ctx;
    if (errorType === 'not_a_function') {
      return ['Symbol is not a function.'];
    }
    if (errorType === 'timeout') {
      return [`Depth=${depth} caused timeout.`];
    }
    return [];
  },
};
