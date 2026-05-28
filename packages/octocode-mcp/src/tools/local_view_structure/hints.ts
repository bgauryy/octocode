/**
 * Response-state hints for localViewStructure.
 *
 * Only emits hints conditional on the response itself.
 *
 * @module tools/local_view_structure/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const extension = typeof c.extension === 'string' ? c.extension : undefined;
    const pattern = typeof c.pattern === 'string' ? c.pattern : undefined;
    const filters: string[] = [];
    if (extension) filters.push(`extension="${extension}"`);
    if (pattern) filters.push(`pattern="${pattern}"`);
    if (filters.length > 0) {
      return [
        `No entries in ${path ?? 'this directory'} matching ${filters.join(' + ')}.`,
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit' && ctx.entryCount) {
      return [`Directory has ${ctx.entryCount} entries.`];
    }
    return [];
  },
};
