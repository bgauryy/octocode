/**
 * Response-state hints for localSearchCode (ripgrep).
 *
 * Empty branch is query-shape aware: inspects pattern length, filters in
 * play (type/include/path/excludeDir), and case sensitivity to propose
 * the most likely-helpful next move.
 *
 * @module tools/local_ripgrep/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const type = typeof c.type === 'string' ? c.type : undefined;
    const include = Array.isArray(c.include) ? (c.include as unknown[]) : [];
    const excludeDir = Array.isArray(c.excludeDir)
      ? (c.excludeDir as unknown[])
      : [];

    const filters: string[] = [];
    if (type) filters.push(`type="${type}"`);
    if (include.length > 0) filters.push(`include=${JSON.stringify(include)}`);
    if (excludeDir.length > 0)
      filters.push(`excludeDir=${JSON.stringify(excludeDir)}`);

    if (filters.length > 0) {
      return [
        `No matches in ${path ?? 'this scope'} with ${filters.join(' + ')}.`,
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const count = ctx.matchCount ? ` (${ctx.matchCount} matches)` : '';
      return [`Too many results${count}. Narrow pattern or scope.`];
    }
    return [];
  },
};
