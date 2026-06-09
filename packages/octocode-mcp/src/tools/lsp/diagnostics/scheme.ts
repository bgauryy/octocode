import { z } from 'zod';
import {
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../../scheme/localSchemaOverlay.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';

// All field descriptions come from the octocode-core ToolSpec via
// withCoreSchemaDescriptions — do not add local .describe() text here.
const DiagnosticsObjectSchema = withCoreSchemaDescriptions(
  LSP_GET_DIAGNOSTICS_TOOL_NAME,
  z.object({
    ...optionalMetaFields,
    uri: z.string().optional(),
    filePath: z.string().optional(),
    workspaceRoot: z.string().optional(),
    severity: z
      .enum(['error', 'warning', 'information', 'hint', 'all'])
      .optional()
      .default('all'),
    source: z.string().optional(),
  })
);

export const LspGetDiagnosticsQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  DiagnosticsObjectSchema.superRefine((value, ctx) => {
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
