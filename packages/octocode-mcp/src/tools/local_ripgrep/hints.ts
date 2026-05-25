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
  hasResults: (_ctx: HintContext = {}) => [],

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const out: string[] = [];
    const pattern = typeof c.pattern === 'string' ? c.pattern : undefined;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const type = typeof c.type === 'string' ? c.type : undefined;
    const include = Array.isArray(c.include) ? (c.include as unknown[]) : [];
    const excludeDir = Array.isArray(c.excludeDir)
      ? (c.excludeDir as unknown[])
      : [];
    const fixedString = c.fixedString === true;
    const caseSensitive = c.caseSensitive === true;

    const filters: string[] = [];
    if (type) filters.push(`type="${type}"`);
    if (include.length > 0) filters.push(`include=${JSON.stringify(include)}`);
    if (excludeDir.length > 0)
      filters.push(`excludeDir=${JSON.stringify(excludeDir)}`);

    if (pattern && pattern.length < 4 && !fixedString) {
      out.push(
        `Pattern "${pattern}" is short (${pattern.length} chars) — may miss boundaries. Try a longer pattern or use wholeWord=true.`
      );
    }

    if (filters.length > 0) {
      out.push(
        `No matches in ${path ?? 'this scope'} with ${filters.join(' + ')}. Drop a filter or broaden the pattern.`
      );
    } else if (path) {
      out.push(
        `No matches in '${path}'. Try a parent path or broaden the pattern.`
      );
    } else {
      out.push(
        'No matches. Broaden the pattern, add type/include filters, or try a different path.'
      );
    }

    if (caseSensitive) {
      out.push('caseSensitive=true is active — disable for fuzzier matching.');
    }
    return out;
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const count = ctx.matchCount ? ` (${ctx.matchCount} matches)` : '';
      return [`Too many results${count}. Narrow pattern or scope.`];
    }
    return [];
  },
};
