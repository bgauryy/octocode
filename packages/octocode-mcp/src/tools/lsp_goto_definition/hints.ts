/**
 * Dynamic hints for lspGotoDefinition tool
 * @module tools/lsp_goto_definition/hints
 *
 * API dynamic keys available: multipleDefinitions, externalPackage, symbolNotFound,
 * timeout, functionSymbol, typeOrVariable
 */

import { getMetadataDynamicHints } from '../../hints/static.js';
import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

const TOOL_NAME = 'lspGotoDefinition';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    const { locationCount, hasExternalPackage } = ctx;
    if (locationCount && locationCount > 1) {
      hints.push(`Found ${locationCount} definitions.`);
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'multipleDefinitions'));
    }
    if (hasExternalPackage) {
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'externalPackage'));
    }
    return hints;
  },

  empty: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    const { searchRadius, lineHint, symbolName } = ctx;
    if (searchRadius) {
      hints.push(
        `Searched ±${searchRadius} lines from lineHint=${lineHint}. Adjust hint.`
      );
    }
    if (symbolName) {
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'symbolNotFound'));
    }
    return hints;
  },

  error: (ctx: HintContext = {}) => {
    const { symbolName, lineHint, uri, errorType } = ctx;

    if (errorType === 'symbol_not_found') {
      return [
        `Symbol "${symbolName}" not found at line ${lineHint}.`,
        ...getMetadataDynamicHints(TOOL_NAME, 'symbolNotFound'),
      ];
    }
    if (errorType === 'file_not_found') {
      return [`File not found: ${uri}`];
    }
    if (errorType === 'timeout') {
      return [...getMetadataDynamicHints(TOOL_NAME, 'timeout')];
    }
    return [];
  },
};
