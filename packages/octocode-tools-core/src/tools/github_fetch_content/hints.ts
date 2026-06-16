import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (!path) return [];
    return ["Verify it's a file, not a directory — use `ghViewRepoStructure`."];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const size = typeof c.fileSize === 'number' ? `${c.fileSize}KB ` : '';
      const totalLines =
        typeof c.totalLines === 'number' ? c.totalLines : undefined;
      const tailLine = totalLines ? Math.max(1, totalLines - 200) : undefined;
      const hints: string[] = [
        `Large file ${size}— use startLine+endLine or matchString for a slice.`,
        `Or minify="symbols" for a skeleton index, then startLine/endLine.`,
      ];
      if (tailLine && totalLines) {
        hints.push(`Tail: startLine=${tailLine}, endLine=${totalLines}.`);
      }
      return hints;
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const branch = typeof c.branch === 'string' ? c.branch : undefined;
      return [
        'Verify path with `ghViewRepoStructure`.',
        ...(branch ? ['Omit `branch` to use the default branch.'] : []),
      ];
    }
    if (ctx.isRateLimited) {
      return [
        `GitHub API rate limited.${ctx.retryAfter ? ` Retry after ${ctx.retryAfter}s.` : ' Wait before retrying.'}`,
      ];
    }
    if (ctx.status === 401) return ['GITHUB_TOKEN is missing or expired.'];
    if (ctx.status === 403) return ['Token lacks `repo` scope.'];
    return [];
  },
};
