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

    return [
      `No files match ${filters.join(' + ')} in ${path ?? 'this path'}.`,
      'Widen: remove filters one at a time; use `iname` for case-insensitive glob or `names` for an OR list of patterns.',
      'For content-based search, use `localSearchCode` instead — `localFindFiles` matches metadata only.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const path = typeof c.path === 'string' ? c.path : undefined;
      return [
        `Path '${path ?? 'specified'}' not found.`,
        'Verify the path with `localViewStructure` at the parent directory.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return [
        'Permission denied — check ALLOWED_PATHS configuration; the path may be outside the permitted scope.',
      ];
    }
    return [];
  },
};
