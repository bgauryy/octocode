import { z } from 'zod';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

const RelatedInformationSchema = z.object({
  uri: z.string(),
  line: z.number(),
  message: z.string(),
});

// 1-based line/character (raw LSP 0-based ranges are converted on output).
const DiagnosticEntrySchema = z.object({
  line: z.number(),
  character: z.number(),
  endLine: z.number().optional(),
  severity: z.enum(['error', 'warning', 'information', 'hint']),
  message: z.string(),
  source: z.string().optional(),
  code: z.union([z.string(), z.number()]).optional(),
  relatedInformation: z.array(RelatedInformationSchema).optional(),
});

const LspSchema = z.object({
  serverAvailable: z.boolean(),
  source: z.string().optional(),
});

const DiagnosticsPaginationSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  itemsPerPage: z.number(),
  totalDiagnostics: z.number(),
  hasMore: z.boolean(),
});

const DiagnosticsDataSchema = z.object({
  uri: z.string(),
  lsp: LspSchema,
  // Both omitted when the language server is unavailable — an empty list +
  // zero summary would read as "file is clean".
  diagnostics: z.array(DiagnosticEntrySchema).optional(),
  pagination: DiagnosticsPaginationSchema.optional(),
  summary: z
    .object({
      errors: z.number(),
      warnings: z.number(),
      information: z.number(),
      hints: z.number(),
    })
    .optional(),
  warnings: z.array(z.string()).optional(),
  hints: z.array(z.string()).optional(),
});

export const LspGetDiagnosticsOutputSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  hints: z.array(z.string()).optional(),
  results: z.array(
    z.union([
      z.strictObject({
        id: z.string().min(1),
        status: z.literal('empty'),
        data: DiagnosticsDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        status: z.literal('error'),
        data: ErrorDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        data: DiagnosticsDataSchema,
      }),
    ])
  ),
});
