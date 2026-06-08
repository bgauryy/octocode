import { z } from 'zod';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

const DiagnosticsDataSchema = z.looseObject({
  uri: z.string(),
  lsp: z.unknown(),
  diagnostics: z.array(z.unknown()),
  summary: z.looseObject({
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
