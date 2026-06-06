import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (_ctx: HintContext = {}) => [],

  error: (ctx: HintContext = {}) => {
    const { depth, errorType, symbolName } = ctx;
    if (errorType === 'lsp_unavailable') {
      const sym = symbolName ? `\`${symbolName}\`` : 'the symbol';
      return [
        'No language server available for this file type.',
        `Use \`localSearchCode\` with \`pattern: "${symbolName ?? 'SYMBOL_NAME'}("\` to find call sites for ${sym} textually.`,
        'Then read each caller file with `localGetFileContent` to inspect the call context.',
      ];
    }
    if (errorType === 'not_a_function') {
      return [
        'Symbol is not a function — `lspCallHierarchy` only works on callable symbols.',
        'For non-function usages (types, variables, imports), use `lspFindReferences` instead.',
      ];
    }
    if (errorType === 'timeout') {
      return [
        `Depth=${depth} caused timeout — reduce depth to 1 and trace one direction at a time.`,
      ];
    }
    return [];
  },
};
