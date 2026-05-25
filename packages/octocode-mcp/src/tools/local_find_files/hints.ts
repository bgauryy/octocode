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
  hasResults: (_ctx: HintContext = {}) => [],

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const out: string[] = [];
    const path = typeof c.path === 'string' ? c.path : undefined;
    const name = typeof c.name === 'string' ? c.name : undefined;
    const extension = typeof c.extension === 'string' ? c.extension : undefined;
    const modifiedWithin =
      typeof c.modifiedWithin === 'string' ? c.modifiedWithin : undefined;
    const sizeGreater =
      typeof c.sizeGreater === 'string' ? c.sizeGreater : undefined;
    const sizeLess = typeof c.sizeLess === 'string' ? c.sizeLess : undefined;

    void extension;
    const filters: string[] = [];
    if (name) filters.push(`name="${name}"`);
    if (modifiedWithin) filters.push(`modifiedWithin="${modifiedWithin}"`);
    if (sizeGreater) filters.push(`sizeGreater="${sizeGreater}"`);
    if (sizeLess) filters.push(`sizeLess="${sizeLess}"`);

    if (filters.length === 0) {
      out.push(
        `No files in ${path ?? 'this path'}. Verify the path exists or use a parent directory.`
      );
    } else if (filters.length === 1) {
      out.push(
        `No files match ${filters[0]} in ${path ?? 'this path'}. Try a broader filter or a parent path.`
      );
    } else {
      out.push(
        `No files match ${filters.join(' + ')} in ${path ?? 'this path'}. Drop one filter at a time.`
      );
    }
    return out;
  },

  error: (_ctx: HintContext = {}) => [],
};
