import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { LOCAL_MAX_DEPTH } from '../../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../../scheme/fields.js';

const requiredLineHintField = clampedInt(1, 1_000_000_000);
const orderHintField = clampedInt(0, 100_000).optional();
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  SEMANTIC_CONTENT_TYPES,
} from '../shared/semanticTypes.js';

const SEMANTIC_OUTPUT_FORMATS = ['structured', 'compact'] as const;

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[LSP_GET_SEMANTIC_CONTENT_TOOL_NAME]?.schema,
} as Record<string, string>;

const SemanticContentQueryShape = z.object({
  id: z.string().optional().describe(QUERY_DESCRIPTIONS.id!),
  mainResearchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.mainResearchGoal!),
  researchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.researchGoal!),
  reasoning: z.string().optional().describe(QUERY_DESCRIPTIONS.reasoning!),
  uri: z.string().optional().describe(QUERY_DESCRIPTIONS.uri!),
  type: z
    .enum(SEMANTIC_CONTENT_TYPES)
    .default('definition')
    .describe(QUERY_DESCRIPTIONS.type!),
  symbolName: z
    .string()
    .min(1)
    .optional()
    .describe(QUERY_DESCRIPTIONS.symbolName!),
  lineHint: requiredLineHintField
    .optional()
    .describe(QUERY_DESCRIPTIONS.lineHint!),
  orderHint: orderHintField.describe(QUERY_DESCRIPTIONS.orderHint!),
  depth: clampedInt(0, LOCAL_MAX_DEPTH)
    .optional()
    .describe(QUERY_DESCRIPTIONS.depth!),
  includeDeclaration: z
    .boolean()
    .optional()
    .default(true)
    .describe(QUERY_DESCRIPTIONS.includeDeclaration!),
  groupByFile: z.boolean().optional().describe(QUERY_DESCRIPTIONS.groupByFile!),
  page: relaxedPageNumberField.describe(QUERY_DESCRIPTIONS.page!),
  itemsPerPage: clampedInt(1, 100)
    .optional()
    .describe(QUERY_DESCRIPTIONS.itemsPerPage!),
  contextLines: clampedInt(0, 100)
    .optional()
    .describe(QUERY_DESCRIPTIONS.contextLines!),
  format: z
    .enum(SEMANTIC_OUTPUT_FORMATS)
    .optional()
    .default('structured')
    .describe(QUERY_DESCRIPTIONS.format!),
  workspaceRoot: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.workspaceRoot!),
});

export const LspGetSemanticContentQueryDisplaySchema =
  SemanticContentQueryShape.superRefine((value, ctx) => {
    if (!value.uri)
      ctx.addIssue({
        code: 'custom',
        path: ['uri'],
        message: 'uri is required',
      });
    if (value.type === 'documentSymbols') return;
    if (!value.symbolName)
      ctx.addIssue({
        code: 'custom',
        path: ['symbolName'],
        message: 'symbolName is required unless type is documentSymbols',
      });
    if (!Number.isInteger(value.lineHint))
      ctx.addIssue({
        code: 'custom',
        path: ['lineHint'],
        message: 'lineHint is required unless type is documentSymbols',
      });
  });

export const LspGetSemanticContentQuerySchema =
  LspGetSemanticContentQueryDisplaySchema;

// Bulk uses the plain shape — superRefine runs per-query at execution.
export const BulkLspGetSemanticContentQuerySchema =
  createRelaxedBulkQuerySchema(SemanticContentQueryShape, { maxQueries: 5 });
