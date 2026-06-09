import { z } from 'zod';
import {
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
} from '../../../scheme/localSchemaOverlay.js';
import { LSP_GET_DIAGNOSTICS_TOOL_NAME } from '../shared/semanticTypes.js';

export const LspGetDiagnosticsQuerySchema = z.preprocess(
  normalizeFilePathAlias,
  z
    .object({
      ...optionalMetaFields,
      uri: z
        .string()
        .optional()
        .describe(
          'Required. Absolute file path or file:/// URI of the file to check. Either uri or filePath must be provided. Use this to check a specific file for errors after editing it.'
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
      severity: z
        .enum(['error', 'warning', 'information', 'hint', 'all'])
        .optional()
        .default('all')
        .describe(
          'Filter by severity level. Use "error" to check only blocking errors after an edit. Defaults to "all".'
        ),
      source: z
        .string()
        .optional()
        .describe(
          'Filter diagnostics by their source (e.g. "typescript", "eslint"). Omit to get all sources.'
        ),
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
