import { z } from 'zod';
import { createRelaxedBulkQuerySchema } from '../../../scheme/localSchemaOverlay.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';

export const LspGetDiagnosticsQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  z
    .object({
      id: z.string().optional().describe('Stable query identifier.'),
      uri: z.string().optional(),
      filePath: z
        .string()
        .optional()
        .describe('Alias for uri — pass either, not both'),
      workspaceRoot: z.string().optional(),
      severity: z
        .enum(['error', 'warning', 'information', 'hint', 'all'])
        .optional()
        .default('all'),
      source: z.string().optional(),
      mainResearchGoal: z.string().optional(),
      researchGoal: z.string().optional(),
      reasoning: z.string().optional(),
    })
    .superRefine((value, ctx) => {
      if (!value.uri && !value.filePath) {
        ctx.addIssue({
          code: 'custom',
          path: ['uri'],
          message: 'Either uri or filePath is required',
        });
      }
    })
);

export const BulkLspGetDiagnosticsQuerySchema = createRelaxedBulkQuerySchema(
  LSP_GET_DIAGNOSTICS_TOOL_NAME,
  LspGetDiagnosticsQuerySchema,
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
