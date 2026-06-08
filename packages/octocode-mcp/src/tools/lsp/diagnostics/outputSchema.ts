import { z } from 'zod';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

const PositionSchema = z.object({
  line: z.number(),
  character: z.number(),
});

const RangeSchema = z.object({
  start: PositionSchema,
  end: PositionSchema,
});

const RelatedInformationSchema = z.object({
  location: z.object({
    uri: z.string(),
    range: RangeSchema,
  }),
  message: z.string(),
});

const DiagnosticEntrySchema = z.object({
  range: RangeSchema,
  severity: z.enum(['error', 'warning', 'information', 'hint']),
  message: z.string(),
  source: z.string().optional(),
  code: z.unknown().optional(),
  relatedInformation: z.array(RelatedInformationSchema).optional(),
});

const LspSchema = z.object({
  serverAvailable: z.boolean(),
  source: z.string().optional(),
});

const DiagnosticsDataSchema = z.object({
  uri: z.string(),
  lsp: LspSchema,
  diagnostics: z.array(DiagnosticEntrySchema),
  summary: z.object({
    errors: z.number(),
    warnings: z.number(),
    information: z.number(),
    hints: z.number(),
  }),
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
