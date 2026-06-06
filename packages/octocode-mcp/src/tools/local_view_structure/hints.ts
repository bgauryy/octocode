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

    if (filters.length === 0) return [];

    return [
      `No entries in ${path ?? 'this directory'} matching ${filters.join(' + ')}.`,
      'Remove the `extension` or `pattern` filter to list all entries, then look for the target manually.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit' && ctx.entryCount) {
      return [
        `Directory has ${ctx.entryCount} entries — add depth=1 or an extension filter to narrow the listing.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const c2 = ctx as Record<string, unknown>;
      const path = typeof c2.path === 'string' ? c2.path : undefined;
      return [
        `Path '${path ?? 'specified'}' not found.`,
        'Check the path is absolute or relative to WORKSPACE_ROOT.',
        'Use `localFindFiles` with `name` filter at the parent directory to locate the correct path.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return [
        'Permission denied reading this directory.',
        'Check ALLOWED_PATHS configuration — the path may be outside the permitted scope.',
      ];
    }
    return [];
  },
};
