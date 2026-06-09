import { z } from 'zod';
import {
  createRelaxedBulkQuerySchema,
  clampedInt,
  LOCAL_OVERLAY_MAX_DEPTH,
  optionalMetaFields,
  orderHintField,
  relaxedPageNumberField,
  requiredLineHintField,
  withCoreSchemaDescriptions,
} from '../../../scheme/localSchemaOverlay.js';
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  SEMANTIC_CONTENT_TYPES,
} from '../shared/semanticTypes.js';

// All field descriptions come from the octocode-core ToolSpec via
// withCoreSchemaDescriptions — do not add local .describe() text here.
const SemanticContentObjectSchema = withCoreSchemaDescriptions(
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  z.object({
    ...optionalMetaFields,
    uri: z.string().optional(),
    filePath: z.string().optional(),
    workspaceRoot: z.string().optional(),
    page: relaxedPageNumberField,
    itemsPerPage: clampedInt(1, 100).optional(),
    contextLines: clampedInt(0, 100).optional(),
    type: z.enum(SEMANTIC_CONTENT_TYPES).default('definition'),
    symbolName: z.string().min(1).optional(),
    lineHint: requiredLineHintField.optional(),
    orderHint: orderHintField,
    depth: clampedInt(0, LOCAL_OVERLAY_MAX_DEPTH).optional(),
    includeDeclaration: z.boolean().optional().default(true),
    groupByFile: z.boolean().optional(),
  })
);

export const LspGetSemanticContentQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  SemanticContentObjectSchema.superRefine((value, ctx) => {
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
