import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    if (ctx.filteredAll) {
      return [
        'All references were excluded by include/exclude patterns.',
        'Remove `includePattern` or `excludePattern` to see the full reference set.',
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'lsp_unavailable') {
      const symbolName = ctx.symbolName;
      const sym = symbolName ? `\`${symbolName}\`` : 'the symbol';
      return [
        'No language server available for this file type.',
        `Use \`localSearchCode\` with \`pattern: "${symbolName ?? 'SYMBOL_NAME'}"\` to find textual usages of ${sym} across the workspace.`,
      ];
    }
    if (ctx.errorType === 'not_found') {
      const symbolName = ctx.symbolName;
      return [
        `LSP could not locate the symbol${symbolName ? ` "${symbolName}"` : ''} — lineHint is likely stale (file changed since the line was recorded). Run localSearchCode with the symbol name to get the current line, then retry.`,
      ];
    }
    if (ctx.errorType === 'timeout') {
      return [
        'Reference lookup timed out — try scoping with `includePattern` to a single package, or use `localSearchCode` as a text fallback.',
      ];
    }
    return [];
  },
};
