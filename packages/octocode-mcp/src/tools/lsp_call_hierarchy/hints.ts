/**
 * Dynamic hints for lspCallHierarchy tool
 * @module tools/lsp_call_hierarchy/hints
 *
 * API dynamic keys available: incomingResults, outgoingResults, notAFunction,
 * timeout, entryPoint, leafNode, flowComplete
 */

import { getMetadataDynamicHints } from '../../hints/static.js';
import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

const TOOL_NAME = 'lspCallHierarchy';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    const {
      direction,
      callCount,
      depth,
      currentPage,
      totalPages,
      hasMorePages,
    } = ctx;

    if (direction === 'incoming') {
      hints.push(`Found ${callCount || 'multiple'} callers.`);
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'incomingResults'));
    } else {
      hints.push(`Found ${callCount || 'multiple'} callees.`);
      hints.push(...getMetadataDynamicHints(TOOL_NAME, 'outgoingResults'));
    }

    if (depth && depth > 1) {
      hints.push(`Depth=${depth} showing ${depth}-level call chain.`);
    }

    if (hasMorePages) {
      hints.push(`Page ${currentPage}/${totalPages}.`);
    }

    return hints;
  },

  empty: (_ctx: HintContext = {}) => [],

  error: (ctx: HintContext = {}) => {
    const { depth, errorType } = ctx;

    if (errorType === 'not_a_function') {
      return [...getMetadataDynamicHints(TOOL_NAME, 'notAFunction')];
    }
    if (errorType === 'timeout') {
      return [
        `Depth=${depth} caused timeout.`,
        ...getMetadataDynamicHints(TOOL_NAME, 'timeout'),
      ];
    }
    return [];
  },
};
