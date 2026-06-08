import type {
  HintContext,
  ToolHintGenerators,
} from '../../../types/metadata.js';
import type { SemanticContentType } from '../shared/semanticTypes.js';

export const hints: ToolHintGenerators = {
  empty: (ctx: HintContext = {}) => {
    const symbolName = typeof ctx.symbolName === 'string' ? ctx.symbolName : '';
    const hintContext = ctx as Record<string, unknown>;
    const semanticType =
      typeof hintContext.type === 'string'
        ? (hintContext.type as SemanticContentType)
        : undefined;
    if (!symbolName && !semanticType) return [];

    return [
      symbolName
        ? `No semantic content found for "${symbolName}".`
        : `No semantic content found for type="${semanticType}".`,
      'Re-anchor with localSearchCode, then retry lspGetSemanticContent with the current lineHint.',
    ];
  },
  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'lsp_unavailable') {
      return [
        'Language server unavailable — use localSearchCode/localGetFileContent as a fallback, then retry after dependencies are available.',
      ];
    }
    if (ctx.errorType === 'symbol_not_found') {
      return [
        'Symbol was not found at the requested anchor — rerun localSearchCode for the exact symbol and use its lineHint.',
      ];
    }
    return [];
  },
};

export function semanticHints(
  type: SemanticContentType,
  complete: boolean
): string[] {
  const next: Record<SemanticContentType, string[]> = {
    definition: [
      'Definition found — use type="references", type="callers", or type="callees" next to inspect impact and flow.',
    ],
    references: [
      'References found — run lspGetDiagnostics on impacted files after edits.',
    ],
    callers: [
      'Callers show static incoming call sites only; combine with localSearchCode for dynamic dispatch or framework wiring.',
      'Set depth=2 to trace one level deeper into the call chain.',
      'Set contextLines=3 to include source snippets around each call site.',
    ],
    callees: [
      'Callees show static outgoing calls only; combine with localSearchCode for dynamic imports, callbacks, or event names.',
      'Set depth=2 to trace one level deeper into called functions.',
      'Set contextLines=3 to include source snippets around each call site.',
    ],
    callHierarchy: [
      'Bidirectional call hierarchy is depth-limited and excludes dynamic calls.',
      'Use type="callers" or type="callees" for a focused single-direction view.',
      'Set depth=2 to explore one more level in both directions.',
    ],
    hover: [
      'Hover found — use type="typeDefinition" for declared types or type="implementation" for concrete behavior.',
    ],
    documentSymbols: [
      'Pick a symbol from the outline and rerun with type="definition", type="references", or type="hover" plus lineHint.',
    ],
    typeDefinition: [
      'Type definition found — use type="references" for type impact or type="implementation" for concrete behavior.',
    ],
    implementation: [
      'Implementation found — use type="callers" or type="callees" to inspect runtime flow.',
    ],
  };

  return complete
    ? next[type]
    : [
        'Semantic evidence is incomplete; combine this result with localSearchCode and project verification.',
        ...next[type],
      ];
}
