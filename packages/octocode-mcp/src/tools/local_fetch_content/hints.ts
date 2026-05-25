/**
 * Dynamic hints for localGetFileContent tool
 * @module tools/local_fetch_content/hints
 *
 * API dynamic keys available: largeFile, sourceCode, configOrDocs, needAnalysis
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const hints: (string | undefined)[] = [];
    const c = ctx as Record<string, unknown>;

    if (c.hasMoreContent) {
      if (
        typeof c.endLine === 'number' &&
        typeof c.totalLines === 'number' &&
        c.endLine < c.totalLines
      ) {
        hints.push(
          `More content: use startLine=${c.endLine + 1} to continue (${c.totalLines - c.endLine} lines remaining)`
        );
      }
      if (
        typeof c.nextCharOffset === 'number' &&
        typeof c.totalChars === 'number'
      ) {
        hints.push(
          `Next page: use charOffset=${c.nextCharOffset} (${c.totalChars - c.nextCharOffset} chars remaining)`
        );
      }
      if (hints.length === 0) {
        hints.push('More content available - use charOffset for pagination.');
      }
    }

    if (typeof c.totalLines === 'number' && !c.hasMoreContent && !c.isPartial) {
      hints.push(`Complete file: ${c.totalLines} lines`);
    }

    return hints;
  },

  empty: (_ctx: HintContext = {}) => [],

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit' && ctx.isLarge) {
      return [
        ctx.fileSize
          ? `Large file (~${Math.round(ctx.fileSize * 0.25)}K tokens). Use matchString or startLine/endLine.`
          : 'File too large. Use matchString or startLine/endLine.',
      ];
    }

    if (ctx.errorType === 'pattern_too_broad') {
      return [
        ctx.tokenEstimate
          ? `Pattern too broad (~${ctx.tokenEstimate.toLocaleString()} tokens).`
          : undefined,
      ];
    }

    return [];
  },
};

/**
 * Get adaptive workflow guidance for large files
 * Explains reasoning behind chunking strategies
 */
export function getLargeFileWorkflowHints(): string[] {
  return [
    "Large file: don't read all.",
    'Flow: localGetFileContent matchString → analyze → localSearchCode usages/imports → localGetFileContent related.',
    'Use charLength to paginate if needed.',
    'Avoid fullContent without charLength.',
  ];
}
