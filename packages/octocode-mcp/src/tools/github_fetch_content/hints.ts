/**
 * Response-state hints for githubGetFileContent.
 *
 * Only emits hints conditional on the response (partial content cursor,
 * mutually-exclusive arg errors, size errors, not-found). No static guidance.
 *
 * @module tools/github_fetch_content/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  hasResults: (ctx: HintContext = {}) => {
    const out: string[] = [];
    const c = ctx as Record<string, unknown>;

    // Flag reads from non-canonical paths so callers can't silently quote
    // example/test/docs code as the real implementation. Benchmark Q4 lost a
    // point exactly this way (read React Router example code, treated it as
    // canonical).
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (path) {
      const nonCanonical =
        /(^|\/)(examples?|__tests?__|tests?|docs?|fixtures?|samples?|benchmarks?|e2e)(\/|$)/i;
      if (nonCanonical.test(path)) {
        out.push(
          `⚠️ Path '${path}' is in a NON-CANONICAL location (examples/__tests__/docs/fixtures). Treat contents as illustrative, not the real implementation. For the canonical source, search inside packages/ or src/.`
        );
      }
    }

    if (c.isPartial && typeof c.endLine === 'number') {
      out.push(
        `Partial content ends at line ${c.endLine}. Use startLine=${
          (c.endLine as number) + 1
        } to continue.`
      );
    }
    return out;
  },

  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const path = typeof c.path === 'string' ? c.path : undefined;
    if (path) {
      return [
        `No content for '${path}'. Verify the path with githubViewRepoStructure or list parent dir.`,
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit') {
      const c = ctx as Record<string, unknown>;
      const size = typeof c.fileSize === 'number' ? `${c.fileSize}KB ` : '';
      return [
        `File ${size}exceeds 300KB cap. Use matchString or startLine/endLine.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const c = ctx as Record<string, unknown>;
      const where = typeof c.path === 'string' ? `'${c.path}'` : 'this path';
      const branch =
        typeof c.branch === 'string' ? ` on branch '${c.branch}'` : '';
      return [
        `${where} not found${branch}. Confirm with githubViewRepoStructure.`,
      ];
    }
    return [];
  },
};
