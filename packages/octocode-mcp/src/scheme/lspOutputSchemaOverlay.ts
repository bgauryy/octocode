import { z } from 'zod/v4';
import { ErrorDataSchema } from '@octocodeai/octocode-core';

const ResultIdentitySchema = z.object({
  id: z.string().min(1),
});

const PositionSchema = z
  .object({
    line: z.number(),
    character: z.number(),
  })
  .passthrough();

const RangeSchema = z
  .object({
    start: PositionSchema,
    end: PositionSchema,
  })
  .passthrough();

const CharPaginationSchema = z
  .object({
    currentPage: z.number(),
    totalPages: z.number(),
    hasMore: z.boolean(),
    charOffset: z.number(),
    charLength: z.number(),
    totalChars: z.number(),
  })
  .passthrough();

const LspPaginationSchema = z
  .object({
    currentPage: z.number(),
    totalPages: z.number(),
    totalResults: z.number().optional(),
    hasMore: z.boolean(),
    resultsPerPage: z.number().optional(),
  })
  .passthrough();

const CallHierarchyItemLocalSchema = z
  .object({
    name: z.string(),
    kind: z.string(),
    uri: z.string(),
    range: RangeSchema,
    content: z.string().optional(),
    selectionRange: RangeSchema.optional(),
    displayRange: z.unknown().optional(),
  })
  .passthrough();

const IncomingCallLocalSchema = z
  .object({
    from: CallHierarchyItemLocalSchema,
    fromRanges: z.array(RangeSchema),
  })
  .passthrough();

const OutgoingCallLocalSchema = z
  .object({
    to: CallHierarchyItemLocalSchema,
    fromRanges: z.array(RangeSchema),
  })
  .passthrough();

const LspCallHierarchyDataLocalSchema = z
  .object({
    item: CallHierarchyItemLocalSchema.optional(),
    incomingCalls: z.array(IncomingCallLocalSchema).optional(),
    outgoingCalls: z.array(OutgoingCallLocalSchema).optional(),
    calls: z.array(z.unknown()).optional(),
    pagination: LspPaginationSchema.optional(),
    outputPagination: CharPaginationSchema.optional(),
    direction: z.enum(['incoming', 'outgoing']).optional(),
    depth: z.number().optional(),
    lspMode: z.enum(['semantic', 'fallback']).optional(),
    hints: z.array(z.string()).optional(),
    error: z.string().optional(),
    errorType: z.string().optional(),
    errorCode: z.string().optional(),
    resolvedPath: z.string().optional(),
    cwd: z.string().optional(),
    searchRadius: z.number().optional(),
  })
  .passthrough();

/**
 * Local output schema for lspCallHierarchy.
 *
 * The runtime returns rich call-hierarchy context for both `hasResults` and
 * `empty` responses (target item, direction, depth, call edges, pagination,
 * hints). The upstream bulk envelope can be stricter than this package's
 * runtime shape, so this overlay makes the advertised MCP output contract
 * match the actual local result contract.
 */
export const LspCallHierarchyOutputLocalSchema = z
  .object({
    format: z.literal('tsv').optional(),
    columns: z.array(z.string()).optional(),
    rows: z.string().optional(),
    hints: z.array(z.string()).optional(),
    results: z.array(
      z.discriminatedUnion('status', [
        ResultIdentitySchema.extend({
          status: z.literal('hasResults'),
          data: LspCallHierarchyDataLocalSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          status: z.literal('empty'),
          data: LspCallHierarchyDataLocalSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          status: z.literal('error'),
          data: ErrorDataSchema,
        }).strict(),
      ])
    ),
    responsePagination: CharPaginationSchema.optional(),
  })
  .strict();
