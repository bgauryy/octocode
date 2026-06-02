import { z } from 'zod/v4';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

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

const ReferenceLocationLocalSchema = z
  .object({
    uri: z.string(),
    range: RangeSchema,
    content: z.string().optional(),
    isDefinition: z.boolean().optional(),
    symbolKind: z.string().optional(),
    displayRange: z.unknown().optional(),
  })
  .passthrough();

const ReferencesByFileLocalSchema = z
  .object({
    uri: z.string(),
    count: z.number(),
    firstLine: z.number(),
    firstCharacter: z.number(),
    hasDefinition: z.boolean().optional(),
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

const LspFindReferencesDataLocalSchema = z
  .object({
    locations: z.array(ReferenceLocationLocalSchema).optional(),
    references: z.array(ReferenceLocationLocalSchema).optional(),
    byFile: z.array(ReferencesByFileLocalSchema).optional(),
    totalReferences: z.number().optional(),
    totalFiles: z.number().optional(),
    pagination: LspPaginationSchema.optional(),
    hasMultipleFiles: z.boolean().optional(),
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
 * Local output schema for lspFindReferences.
 *
 * `groupByFile:true` intentionally compacts `locations[]` and exposes the
 * ranked per-file rollup as structured `byFile[]` instead of burying it in
 * hints. The upstream schema does not know about that local product mode.
 */
export const LspFindReferencesOutputLocalSchema = z
  .object({
    base: z.string().optional(),
    shared: z
      .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
      .optional(),
    hints: z.array(z.string()).optional(),
    results: z.array(
      z.union([
        ResultIdentitySchema.extend({
          status: z.literal('empty'),
          data: LspFindReferencesDataLocalSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          status: z.literal('error'),
          data: ErrorDataSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          data: LspFindReferencesDataLocalSchema,
        }).strict(),
      ])
    ),
    responsePagination: CharPaginationSchema.optional(),
  })
  .strict();

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
    base: z.string().optional(),
    shared: z
      .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
      .optional(),
    hints: z.array(z.string()).optional(),
    results: z.array(
      // Three variants: empty / error / hasResults. The hasResults variant
      // is signaled by an ABSENT `status` field (see bulk runner), so the
      // union order matters: the empty/error variants must come first so
      // they match before the catch-all hasResults variant accepts.
      z.union([
        ResultIdentitySchema.extend({
          status: z.literal('empty'),
          data: LspCallHierarchyDataLocalSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          status: z.literal('error'),
          data: ErrorDataSchema,
        }).strict(),
        ResultIdentitySchema.extend({
          data: LspCallHierarchyDataLocalSchema,
        }).strict(),
      ])
    ),
    responsePagination: CharPaginationSchema.optional(),
  })
  .strict();
