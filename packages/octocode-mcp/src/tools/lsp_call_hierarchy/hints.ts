import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const c = ctx as Record<string, unknown>;
    const symbolName =
      typeof c.symbolName === 'string' ? c.symbolName : undefined;
    const direction = typeof c.direction === 'string' ? c.direction : undefined;
    if (!symbolName) return [];
    const dirLabel = direction === 'outgoing' ? 'calls made by' : 'callers of';
    return [
      `No call hierarchy found for ${dirLabel} "${symbolName}".`,
      direction === 'outgoing'
        ? `"${symbolName}" may make no calls, or the LSP couldn't resolve its body — try lspFindReferences to locate usages instead.`
        : `"${symbolName}" is not called anywhere in the indexed workspace. It may be an entry point, exported API, or only called dynamically. Use localSearchCode with pattern="${symbolName}" to confirm.`,
    ];
  },

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
