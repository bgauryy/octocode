import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const { searchRadius, lineHint, symbolName, uri } = ctx;
    const location = uri ? ` in ${uri}` : '';
    if (searchRadius) {
      return [
        `"${symbolName}" not anchored within ±${searchRadius} lines of lineHint=${lineHint}${location}.`,
        'Re-anchor: run `localSearchCode` with the exact symbol name to get the real line number, then retry with that lineHint.',
        'If it is an import, `lspGotoDefinition` on the import line resolves the re-export chain first.',
      ];
    }
    if (symbolName) {
      return [
        `Definition not found for "${symbolName}"${location}.`,
        'Verify the symbol name is exact (no parens, no partials) and lineHint points to a usage or definition line.',
      ];
    }
    return [];
  },

  error: (ctx: HintContext = {}) => {
    const { symbolName, lineHint, uri, errorType } = ctx;
    if (errorType === 'symbol_not_found') {
      return [
        `"${symbolName}" not found at line ${lineHint} — lineHint is likely stale (file changed since the line number was recorded).`,
        'Re-anchor: run localSearchCode with the exact symbol name to get the current line number, then retry with that lineHint.',
      ];
    }
    if (errorType === 'file_not_found') {
      return [
        `File not found: ${uri}`,
        'Verify the path with `localViewStructure` or `localFindFiles`, then retry.',
      ];
    }
    if (errorType === 'timeout') {
      return [
        'Definition lookup timed out — retry once; if it persists use `localSearchCode` with the symbol name as a text fallback.',
      ];
    }
    return [];
  },
};
