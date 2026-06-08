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
  const found: Record<SemanticContentType, string[]> = {
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

  const notFound: Partial<Record<SemanticContentType, string[]>> = {
    definition: [
      'No definition found. Verify the symbol name and lineHint are correct (rerun localSearchCode); ensure project dependencies are installed so the language server can resolve imports.',
    ],
    hover: [
      'No hover content returned. Try type="definition" to locate the symbol, or verify the language server supports this file type.',
    ],
    typeDefinition: [
      'No type definition found. The expression may have an inferred or primitive type with no explicit declaration; try type="hover" for the inferred type signature.',
    ],
    callers: [
      'No callers found. The function may be an entry point, an exported API called externally, or invoked through dynamic dispatch (events, callbacks, framework injection).',
      'Use localSearchCode with the function name as a pattern to find string-based or dynamic references.',
    ],
    callees: [
      'No callees found. The function may contain only direct property access or primitive operations with no trackable call sites.',
      'Use localSearchCode for pattern-based search if the function uses dynamic method calls.',
    ],
    callHierarchy: [
      'No calls found in either direction. The function may be isolated or all call sites are dynamic.',
      'Use localSearchCode with the function name to find string-based references and cross-check.',
    ],
    documentSymbols: [
      'No symbols returned. The language server may be unavailable for this file type.',
      'Use localSearchCode with patterns like "export|function|class|const" as a fallback outline.',
    ],
    implementation: [
      'No implementations found. symbolName must be a method or property declared inside an interface or abstract class — not the interface/class name itself.',
      'Use type="documentSymbols" to list the interface members, then retry with a member name as symbolName.',
    ],
  };

  if (complete) return found[type];

  return [
    'Semantic evidence is incomplete; combine this result with localSearchCode and project verification.',
    ...(notFound[type] ?? found[type]),
  ];
}
