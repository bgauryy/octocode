/**
 * Dynamic hints for localSearchCode (ripgrep) tool
 * @module tools/local_ripgrep/hints
 *
 * API dynamic keys available: largeResult, functionFound, typeOrVariableFound,
 * multipleMatches, noLineNumbers
 */

import { getMetadataDynamicHints } from '../../hints/static.js';
import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

const TOOL_NAME = 'localSearchCode';

/**
 * Filter out undefined values from hints array.
 * Ensures clean hint arrays without null/undefined entries.
 */
function filterValidHints(hints: (string | undefined)[]): string[] {
  return hints.filter((h): h is string => h !== undefined && h !== null);
}

export const hints: ToolHintGenerators = {
  hasResults: (_ctx: HintContext = {}) => [],

  empty: (_ctx: HintContext = {}) => [],

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const hints = [
        `Too many results${ctx.matchCount ? ` (${ctx.matchCount} matches)` : ''}. Narrow pattern/scope.`,
        ...getMetadataDynamicHints(TOOL_NAME, 'largeResult'),
      ];
      return filterValidHints(hints);
    }
    return [];
  },
};
