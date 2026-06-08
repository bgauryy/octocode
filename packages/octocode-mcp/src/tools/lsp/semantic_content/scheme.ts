import { z } from 'zod';
import {
  contextLinesField,
  createRelaxedBulkQuerySchema,
  clampedInt,
  depthField,
  orderHintField,
  relaxedPageNumberField,
  requiredLineHintField,
} from '../../../scheme/localSchemaOverlay.js';
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  SEMANTIC_CONTENT_TYPES,
} from '../shared/semanticTypes.js';

const baseFields = {
  id: z.string().optional().describe('Stable query identifier.'),
  uri: z.string().optional(),
  filePath: z
    .string()
    .optional()
    .describe('Alias for uri — pass either, not both'),
  workspaceRoot: z.string().optional(),
  page: relaxedPageNumberField,
  itemsPerPage: clampedInt(1, 100)
    .optional()
    .describe(
      'Semantic items per page for documentSymbols and call-flow results. Defaults to 40 for documentSymbols and 10 for call-flow.'
    ),
  contextLines: contextLinesField,
  mainResearchGoal: z.string().optional(),
  researchGoal: z.string().optional(),
  reasoning: z.string().optional(),
};

export const LspGetSemanticContentQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  z
    .object({
      ...baseFields,
      type: z.enum(SEMANTIC_CONTENT_TYPES).default('definition'),
      symbolName: z.string().min(1).optional(),
      lineHint: requiredLineHintField
        .optional()
        .describe(
          'Required for all symbol-anchored types (everything except documentSymbols). 1-based line number from a prior localSearchCode result.'
        ),
      orderHint: orderHintField,
      depth: depthField,
      includeDeclaration: z
        .boolean()
        .optional()
        .default(true)
        .describe(
          'For references: whether to include the symbol declaration itself in results. Defaults to true.'
        ),
      groupByFile: z.boolean().optional(),
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
