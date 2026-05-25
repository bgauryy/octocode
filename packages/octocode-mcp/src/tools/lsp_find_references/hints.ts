/**
 * Response-state hints for lspFindReferences.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/lsp_find_references/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const { locationCount, fileCount, hasMultipleFiles } = ctx;

    if (locationCount && locationCount > 20) {
      out.push(`Found ${locationCount} references.`);
    }
    if (hasMultipleFiles) {
      out.push(`References span ${fileCount || 'multiple'} files.`);
    }
    return out;
  },

  empty: (ctx: HintContext = {}) => {
    if (ctx.filteredAll) {
      return [
        'All references were excluded by file patterns. Broaden include/exclude.',
      ];
    }
    return [];
  },

  error: (_ctx: HintContext = {}) => [],
};
