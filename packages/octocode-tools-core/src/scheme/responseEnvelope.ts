import { z } from 'zod';
import { ToolContinuationSchema, ToolDiagnosticSchema } from './pagination.js';

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
  base: z.string().optional(),

  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),

  responsePagination: ResponsePaginationSchema,
} as const;

export function withResponseEnvelope<S extends z.ZodObject>(schema: S): S {
  return schema.extend(responseEnvelopeFields) as unknown as S;
}

// ---------------------------------------------------------------------------
// Universal result row — every tool result array should contain rows shaped
// like this (with tool-specific `data` payloads).
// ---------------------------------------------------------------------------

export function makeToolResultRowSchema<TData extends z.ZodTypeAny>(
  dataSchema: TData
) {
  return z.object({
    id: z.string(),
    status: z.enum(['empty', 'error']).optional(),
    data: dataSchema,
    diagnostics: z.array(ToolDiagnosticSchema).optional(),
    next: z.record(z.string(), ToolContinuationSchema).optional(),
  });
}

// Generic row with passthrough data — useful for schema declarations that
// just need to assert the envelope shape without constraining the payload.
export const GenericToolResultRowSchema = makeToolResultRowSchema(
  z.record(z.string(), z.unknown()).optional()
);

// Shared envelope fields present on every BulkToolOutput.
export const bulkOutputEnvelopeFields = {
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  responsePagination: ResponsePaginationSchema,
} as const;
