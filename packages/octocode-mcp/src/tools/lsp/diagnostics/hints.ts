import type {
  HintContext,
  ToolHintGenerators,
} from '../../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const uri = typeof ctx.uri === 'string' ? ` for ${ctx.uri}` : '';
    if (!uri) return [];
    return [`No diagnostics returned${uri}.`];
  },
  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'lsp_unavailable') {
      return [
        'Language server unavailable — run project lint/typecheck/tests directly for diagnostics.',
      ];
    }
    return [];
  },
};

export function diagnosticHints(
  _source: string,
  empty: boolean,
  errorCount = 0,
  filteredOutCount = 0,
  pagination?: {
    currentPage: number;
    totalPages: number;
    totalDiagnostics: number;
  }
): string[] {
  const emptyHint =
    empty && filteredOutCount > 0
      ? `No diagnostics matched (filter active — ${filteredOutCount} diagnostics at other severities/sources exist; use severity="all").`
      : empty
        ? 'No LSP diagnostics found.'
        : 'LSP diagnostics returned.';
  const base = [
    emptyHint,
    'Still run lint/typecheck/tests to fully verify changes.',
  ];

  if (!empty && errorCount > 0) {
    base.push(
      'Use lspGetSemanticContent type="definition" on impacted symbols to trace errors.'
    );
  }

  if (pagination && pagination.currentPage < pagination.totalPages) {
    base.push(
      `Page ${pagination.currentPage}/${pagination.totalPages} (${pagination.totalDiagnostics} total diagnostics). Use page=${pagination.currentPage + 1} to continue.`
    );
  }

  return base;
}
