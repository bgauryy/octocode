/**
 * Response-state hints for localFindFiles.
 *
 * Empty branch names the actual filters that produced zero results (name
 * pattern, extension, modifiedWithin window, etc.) and proposes a single
 * concrete recovery move.
 *
 * @module tools/local_find_files/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    const name = typeof c.name === 'string' ? c.name : undefined;
    const modifiedWithin =
      typeof c.modifiedWithin === 'string' ? c.modifiedWithin : undefined;
    const sizeGreater =
      typeof c.sizeGreater === 'string' ? c.sizeGreater : undefined;
    const sizeLess = typeof c.sizeLess === 'string' ? c.sizeLess : undefined;

    const filters: string[] = [];
    if (name) filters.push(`name="${name}"`);
    if (modifiedWithin) filters.push(`modifiedWithin="${modifiedWithin}"`);
    if (sizeGreater) filters.push(`sizeGreater="${sizeGreater}"`);
    if (sizeLess) filters.push(`sizeLess="${sizeLess}"`);

    if (filters.length === 0) return [];
    return [`No files match ${filters.join(' + ')} in ${path ?? 'this path'}.`];
  },

  error: (_ctx: HintContext = {}) => [],
};
