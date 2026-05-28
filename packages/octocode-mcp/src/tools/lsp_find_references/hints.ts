/**
 * Response-state hints for lspFindReferences.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/lsp_find_references/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    if (ctx.filteredAll) {
      return ['All references were excluded by include/exclude patterns.'];
    }
    return [];
  },

  error: () => [],
};
