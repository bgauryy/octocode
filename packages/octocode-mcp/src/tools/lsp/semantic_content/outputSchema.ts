import { z } from 'zod';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

const SemanticDataSchema = z.looseObject({
  type: z.string(),
  uri: z.string(),
  resolvedSymbol: z.unknown().optional(),
  lsp: z.unknown(),
  evidence: z.unknown(),
  payload: z.looseObject({
    kind: z.string(),
  }),
  pagination: z.unknown().optional(),
  warnings: z.array(z.string()).optional(),
  hints: z.array(z.string()).optional(),
});

export const LspGetSemanticContentOutputSchema = z.object({
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
        data: SemanticDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        status: z.literal('error'),
        data: ErrorDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        data: SemanticDataSchema,
      }),
    ])
  ),
});
