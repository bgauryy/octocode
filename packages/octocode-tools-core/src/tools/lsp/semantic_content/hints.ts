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
    if (semanticType === 'documentSymbols') {
      return ['Verify uri with localFindFiles/localSearchCode, then retry.'];
    }

    return [
      'Re-anchor with localSearchCode to get uri+symbolName+lineHint; use documentSymbols if the symbol is ambiguous.',
    ];
  },
  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'lsp_unavailable') {
      return [
        'Language server unavailable — use localSearchCode for references and localGetFileContent for source slices.',
      ];
    }
    if (ctx.errorType === 'symbol_not_found') {
      return [
        'Run localSearchCode for the exact symbol line, then retry with updated uri+lineHint.',
      ];
    }
    return [];
  },
};

export function semanticHints(
  type: SemanticContentType,
  complete: boolean
): string[] {
  if (complete) {
    const success: Partial<Record<SemanticContentType, string[]>> = {
      documentSymbols: [
        'Use returned line values as lineHint for definition, references, or callers.',
      ],
      hover: [
        'type="definition" to jump to source, type="references" for usage.',
      ],
      definition: [
        'localGetFileContent for context, type="references" or "callHierarchy" for impact.',
      ],
      typeDefinition: [
        'localGetFileContent for the type, type="implementation" for concrete implementations.',
      ],
      implementation: [
        'localGetFileContent for implementation, type="references" for call sites.',
      ],
      references: [
        'groupByFile=true for compact summary, localGetFileContent for context.',
      ],
      callers: [
        'Increase depth for a wider tree, localGetFileContent for context.',
      ],
      callees: [
        'Increase depth, type="definition" on returned calls for source.',
      ],
      callHierarchy: [
        'Increase depth, localGetFileContent on call sites for context.',
      ],
    };
    return [...(success[type] ?? [])];
  }

  const notFound: Partial<Record<SemanticContentType, string[]>> = {
    definition: ['Re-anchor with localSearchCode and retry.'],
    hover: ['Try type="definition" instead.'],
    typeDefinition: ['Try type="hover" for the inferred type.'],
    callers: ['Use localSearchCode for dynamic references.'],
    callees: ['Use localSearchCode for dynamic calls.'],
    callHierarchy: ['Use localSearchCode for dynamic references.'],
    documentSymbols: [
      'Use localSearchCode with "export|function|class|const" as a fallback.',
    ],
    implementation: [
      'symbolName must be a method/property of an interface or abstract class — not the class name.',
      'Use type="documentSymbols" to list members, then retry with a member name.',
    ],
  };

  return [...(notFound[type] ?? [])];
}
