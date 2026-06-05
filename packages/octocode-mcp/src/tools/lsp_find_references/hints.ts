/**
 * Response-state hints for lspFindReferences.
 *
 * Emits actionable recovery moves the agent can execute immediately.
 *
 * @module tools/lsp_find_references/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    if (ctx.filteredAll) {
      return [
        'All references were excluded by include/exclude patterns.',
        'Remove `includePattern` or `excludePattern` to see the full reference set.',
      ];
    }
    const symbolName = ctx.symbolName;
    if (symbolName) {
      return [
        `No semantic references found for '${symbolName}'.`,
        'If no language server is available for this file type, use `localSearchCode` with ' +
          `\`pattern: "${symbolName}"\` to find textual usages across the workspace.`,
        'Verify `lineHint` points to the exact line where the symbol is defined — use `localSearchCode` to confirm the line number first.',
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'timeout') {
      return [
        'Reference lookup timed out — try scoping with `includePattern` to a single package, or use `localSearchCode` as a text fallback.',
      ];
    }
    if (ctx.errorType === 'not_found') {
      const symbolName = ctx.symbolName;
      return [
        `LSP could not locate the symbol${symbolName ? ` "${symbolName}"` : ''} — fall back to \`localSearchCode\` with the symbol name as a text pattern.`,
      ];
    }
    return [];
  },
};
