import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const names =
      Array.isArray(c.names) && c.names.length > 0
        ? (c.names as string[])
        : undefined;
    const modifiedWithin =
      typeof c.modifiedWithin === 'string' ? c.modifiedWithin : undefined;
    const sizeGreater =
      typeof c.sizeGreater === 'string' ? c.sizeGreater : undefined;
    const sizeLess = typeof c.sizeLess === 'string' ? c.sizeLess : undefined;

    const filters: string[] = [];
    if (names) filters.push(`names=${JSON.stringify(names)}`);
    if (modifiedWithin) filters.push(`modifiedWithin="${modifiedWithin}"`);
    if (sizeGreater) filters.push(`sizeGreater="${sizeGreater}"`);
    if (sizeLess) filters.push(`sizeLess="${sizeLess}"`);

    if (filters.length === 0) return [];

    return [
      `No metadata match for ${filters.join(', ')} — remove one filter or broaden names.`,
      'For content text, switch to `localSearchCode`.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'not_found') {
      return ['Verify path with `localViewStructure` at the parent directory.'];
    }
    if (ctx.errorType === 'permission') {
      return ['Permission denied — check ALLOWED_PATHS configuration.'];
    }
    return [];
  },
};
