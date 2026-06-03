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
    return [
      'No references found.',
      'Use `localSearchCode` to find textual usages of the symbol if LSP is unavailable for this language.',
    ];
  },

  error: () => [],
};
