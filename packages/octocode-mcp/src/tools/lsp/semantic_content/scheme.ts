import { z } from 'zod';
import {
  contextLinesField,
  createRelaxedBulkQuerySchema,
  clampedInt,
  depthField,
  optionalMetaFields,
  orderHintField,
  relaxedPageNumberField,
  requiredLineHintField,
} from '../../../scheme/localSchemaOverlay.js';
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  SEMANTIC_CONTENT_TYPES,
} from '../shared/semanticTypes.js';

const baseFields = {
  ...optionalMetaFields,
  uri: z
    .string()
    .optional()
    .describe(
      'Required for all query types. Absolute file URI or path of the file containing the symbol. Pass either uri or filePath, not both.'
    ),
  filePath: z
    .string()
    .optional()
    .describe(
      'Alias for uri — pass either uri or filePath, not both. Required when uri is omitted.'
    ),
  workspaceRoot: z
    .string()
    .optional()
    .describe(
      'Override the workspace root used to locate/start the language server. Omit to auto-detect from the file path.'
    ),
  page: relaxedPageNumberField.describe(
    'Result page (1-based) for documentSymbols and call-flow results. Use page=2, page=3, … to walk through long results.'
  ),
  itemsPerPage: clampedInt(1, 100)
    .optional()
    .describe(
      'Semantic items per page for documentSymbols and call-flow results. Defaults to 40 for documentSymbols and 10 for call-flow.'
    ),
  contextLines: contextLinesField,
};

export const LspGetSemanticContentQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  z
    .object({
      ...baseFields,
      type: z
        .enum(SEMANTIC_CONTENT_TYPES)
        .default('definition')
        .describe(
          'Semantic query kind: definition, references, callers, callees, callHierarchy, hover, documentSymbols, typeDefinition, or implementation. Defaults to definition.'
        ),
      symbolName: z
        .string()
        .min(1)
        .optional()
        .describe(
          'Required unless type is documentSymbols. Exact name of the symbol to resolve at lineHint — case-sensitive, no parentheses.'
        ),
      lineHint: requiredLineHintField
        .optional()
        .describe(
          'Required unless type is documentSymbols. 1-based line number of the symbol from a prior localSearchCode result. The LSP searches ±2 lines around this hint.'
        ),
      orderHint: orderHintField.describe(
        'When multiple occurrences of symbolName sit on lineHint, select the Nth (0-based) occurrence. Defaults to 0 (first match).'
      ),
      depth: depthField.describe(
        'For callHierarchy/callers/callees: maximum recursion depth to expand the call tree. Max 20; keep shallow to stay readable.'
      ),
      includeDeclaration: z
        .boolean()
        .optional()
        .default(true)
        .describe(
          'For references: whether to include the symbol declaration itself in results. Defaults to true.'
        ),
      groupByFile: z
        .boolean()
        .optional()
        .describe(
          'For references: return a compact per-file summary instead of a flat list of usages.'
        ),
    })
    .superRefine((value, ctx) => {
      requireUriOrFilePath(value, ctx);
      if (value.type === 'documentSymbols') {
        return;
      }
      if (!value.symbolName) {
        ctx.addIssue({
          code: 'custom',
          path: ['symbolName'],
          message: 'symbolName is required unless type is documentSymbols',
        });
      }
      if (!Number.isInteger(value.lineHint)) {
        ctx.addIssue({
          code: 'custom',
          path: ['lineHint'],
          message: 'lineHint is required unless type is documentSymbols',
        });
      }
    })
);

export const BulkLspGetSemanticContentQuerySchema =
  createRelaxedBulkQuerySchema(
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
    LspGetSemanticContentQuerySchema,
    { maxQueries: 5 }
  );

function normalizeFilePathAlias(value: unknown): unknown {
  if (
    value &&
    typeof value === 'object' &&
    'filePath' in value &&
    !('uri' in value)
  ) {
    const { filePath, ...rest } = value as { filePath?: string };
    return { ...rest, uri: filePath };
  }
  return value;
}

function requireUriOrFilePath(
  value: { uri?: string; filePath?: string },
  ctx: z.RefinementCtx
): void {
  if (!value.uri && !value.filePath) {
    ctx.addIssue({
      code: 'custom',
      path: ['uri'],
      message: 'Either uri or filePath is required',
    });
  }
}
