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
  // On a successful result we deliberately emit NO coaching hints. The
  // "what to query next" chains (type="references"/"callers"/"callees", depth,
  // contextLines) and the typical research flow are already spelled out in the
  // tool description (TYPES + FLOW). Repeating them on every successful call —
  // including back-to-back repeat calls — is pure token waste.
  if (complete) return [];

  // Incomplete / not-found results keep targeted recovery guidance: this is
  // context-specific (why nothing was found + how to re-anchor) and is not
  // covered by the static tool description.
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

  return [
    'Semantic evidence is incomplete; combine this result with localSearchCode and project verification.',
    ...(notFound[type] ?? []),
  ];
}
