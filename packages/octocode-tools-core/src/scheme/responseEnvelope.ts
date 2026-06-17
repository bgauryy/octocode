import { z } from 'zod';

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

  responsePagination: ResponsePaginationSchema,
} as const;

export function withResponseEnvelope<S extends z.ZodObject>(schema: S): S {
  return schema.extend(responseEnvelopeFields) as unknown as S;
}
