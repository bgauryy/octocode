/**
 * Response-state hints for lspGotoDefinition.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/lsp_goto_definition/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const { locationCount } = ctx;
    if (locationCount && locationCount > 1) {
      return [`Found ${locationCount} definitions.`];
    }
    return [];
  },

  empty: (ctx: HintContext = {}) => {
    const { searchRadius, lineHint } = ctx;
    if (searchRadius) {
      return [
        `Searched ±${searchRadius} lines from lineHint=${lineHint}. Adjust hint.`,
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    const { symbolName, lineHint, uri, errorType } = ctx;

    if (errorType === 'symbol_not_found') {
      return [`Symbol "${symbolName}" not found at line ${lineHint}.`];
    }
    if (errorType === 'file_not_found') {
      return [`File not found: ${uri}`];
    }
    if (errorType === 'timeout') {
      return ['Definition lookup timed out.'];
    }
    return [];
  },
};
