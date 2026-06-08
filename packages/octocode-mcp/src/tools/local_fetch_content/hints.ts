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
      const totalLines =
        typeof c.totalLines === 'number' ? c.totalLines : undefined;
      const tailLine = totalLines ? Math.max(1, totalLines - 200) : undefined;
      const hints: string[] = [
        `File${kb} exceeds the read budget — use matchString or startLine+endLine for a focused section.`,
        `Or signaturesOnly=true for an export index (80–95% fewer chars), then startLine/endLine for a body.`,
      ];
      if (tailLine && totalLines) {
        hints.push(
          `File has ${totalLines} total lines. To read the tail: startLine=${tailLine}, endLine=${totalLines}.`
        );
      }
      return hints;
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
