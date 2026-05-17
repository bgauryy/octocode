/**
 * Dynamic hints for lspFindReferences tool
 * @module tools/lsp_find_references/hints
 *
 * API dynamic keys available: manyReferences, multipleFiles, pagination,
 * functionSymbol, impactAnalysis, deadCode
 */

import { getMetadataDynamicHints } from '../../hints/static.js';
import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

const TOOL_NAME = 'lspFindReferences';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    const { locationCount, fileCount, hasMultipleFiles } = ctx;

    if (locationCount && locationCount > 20) {
      hints.push(`Found ${locationCount} references.`);
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'manyReferences'));
    }
    if (hasMultipleFiles) {
      hints.push(`References span ${fileCount || 'multiple'} files.`);
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'multipleFiles'));
    }
    return hints;
  },

  empty: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    if (ctx.filteredAll) {
      hints.push(
        'All references were excluded by file patterns. Try broader patterns or remove filtering.'
      );
    }
    return hints;
  },

  error: (_ctx: HintContext = {}) => [],
};
