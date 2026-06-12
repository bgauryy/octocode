import { z } from 'zod';

export const EvidenceSchema = z
  .object({
    kind: z
      .enum([
        'metadata',
        'content',
        'structure',
        'code',
        'docs',
        'config',
        'pr',
        'repo',
        'package',
        'definition',
        'references',
        'calls',
      ])
      .optional(),

    answerReady: z.boolean().optional(),

    confidence: z.enum(['high', 'medium', 'low']).optional(),

    complete: z.boolean().optional(),

    reason: z.string().optional(),

    missingFields: z.array(z.string()).optional(),
  })
  .optional();

const ResponsePaginationSchema = z
  .object({
    currentPage: z.number(),
    totalPages: z.number(),
    hasMore: z.boolean(),
    charOffset: z.number(),
    charLength: z.number(),
    totalChars: z.number(),
    nextCharOffset: z.number().optional(),
  })
  .optional();

export const responseEnvelopeFields = {
  hints: z.array(z.string()).optional(),

  base: z.string().optional(),

  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),

  evidence: EvidenceSchema,

  responsePagination: ResponsePaginationSchema,
} as const;

export function withResponseEnvelope<S extends z.ZodObject>(schema: S): S {
  // Zod's .extend() widens the shape type; cast via unknown to preserve the
  // caller's S type variable so downstream schemas stay narrowly typed.
  return schema.extend(responseEnvelopeFields) as unknown as S;
}
