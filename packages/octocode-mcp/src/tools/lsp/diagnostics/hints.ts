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

export function diagnosticHints(source: string, empty: boolean): string[] {
  return [
    empty
      ? `No diagnostics returned from ${source}.`
      : `Diagnostics returned from ${source}.`,
    'Diagnostics are fast semantic evidence; still run project lint/typecheck/tests before claiming a risky change is fully verified.',
  ];
}
