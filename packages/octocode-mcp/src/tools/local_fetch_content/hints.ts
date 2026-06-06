import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (!path) return [];
    return [
      `File '${path}' is empty (zero bytes).`,
      'Verify this is the correct file — use `localFindFiles` with a `name` filter to confirm the path.',
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const kb =
        typeof c.fileSize === 'number'
          ? ` (~${Math.round((c.fileSize as number) / 1024)}KB)`
          : '';
      return [
        `File${kb} exceeds the read budget — use matchString or startLine+endLine for a focused section.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const path = typeof c.path === 'string' ? `'${c.path}'` : 'the file';
      return [
        `${path} not found.`,
        'Use `localFindFiles` with a `name` filter to locate the correct path.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return [
        'Permission denied reading this file.',
        'Check ALLOWED_PATHS configuration — the path may be outside the permitted scope.',
      ];
    }
    return [];
  },
};
