import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (!path) return [];
    return ['Confirm path with `localFindFiles`.'];
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
        `File${kb} too large — use matchString or startLine+endLine for a slice.`,
        `Or minify="symbols" for a skeleton index, then startLine/endLine.`,
      ];
      if (tailLine && totalLines) {
        hints.push(`Tail: startLine=${tailLine}, endLine=${totalLines}.`);
      }
      return hints;
    }
    if (ctx.errorType === 'directory') {
      const c = ctx as Record<string, unknown>;
      const path = typeof c.path === 'string' ? `'${c.path}'` : 'Path';
      return [
        `${path} is a directory — use \`localViewStructure\` to explore it.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      return ['Use `localFindFiles` to locate the correct path.'];
    }
    if (ctx.errorType === 'permission') {
      return ['Permission denied — check ALLOWED_PATHS configuration.'];
    }
    return [];
  },
};
