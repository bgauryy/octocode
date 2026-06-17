import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const extensions = Array.isArray(c.extensions)
      ? c.extensions.filter(
          (value): value is string => typeof value === 'string'
        )
      : [];
    const pattern = typeof c.pattern === 'string' ? c.pattern : undefined;

    const active = [
      ...(pattern ? [`pattern="${pattern}"`] : []),
      ...(extensions.length > 0 ? [`extensions=${extensions.join(',')}`] : []),
    ];
    if (active.length === 0) return [];

    return [
      `No entries matched ${active.join(' + ')} — remove filters or increase depth/recursive.`,
    ];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'not_found') {
      return [
        'Path must be absolute — use localFindFiles to discover the correct path.',
      ];
    }
    if (ctx.errorType === 'permission') {
      return ['Permission denied — check ALLOWED_PATHS configuration.'];
    }
    return [];
  },
};
