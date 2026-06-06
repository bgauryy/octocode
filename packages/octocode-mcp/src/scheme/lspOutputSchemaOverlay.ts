import { z } from 'zod';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';

const PositionSchema = z.looseObject({
  line: z.number(),
  character: z.number(),
});

const RangeSchema = z.looseObject({
  start: PositionSchema,
  end: PositionSchema,
});

const LspPaginationSchema = z.looseObject({
  currentPage: z.number(),
  totalPages: z.number(),
  totalResults: z.number().optional(),
  hasMore: z.boolean(),
  resultsPerPage: z.number().optional(),
});

const ReferenceLocationLocalSchema = z.looseObject({
  uri: z.string(),
  range: RangeSchema,
  content: z.string().optional(),
  isDefinition: z.boolean().optional(),
  symbolKind: z.string().optional(),
  displayRange: z.unknown().optional(),
});

const ReferencesByFileLocalSchema = z.looseObject({
  uri: z.string(),
  count: z.number(),
  firstLine: z.number(),
  firstCharacter: z.number(),
  lines: z.array(z.number()),
  hasDefinition: z.boolean().optional(),
});

const CallHierarchyItemLocalSchema = z.looseObject({
  name: z.string(),
  kind: z.string(),
  uri: z.string(),
  range: RangeSchema,
  content: z.string().optional(),
  selectionRange: RangeSchema.optional(),
  displayRange: z.unknown().optional(),
});

const IncomingCallLocalSchema = z.looseObject({
  from: CallHierarchyItemLocalSchema,
  fromRanges: z.array(RangeSchema),
});

const OutgoingCallLocalSchema = z.looseObject({
  to: CallHierarchyItemLocalSchema,
  fromRanges: z.array(RangeSchema),
});

const LspFindReferencesDataLocalSchema = z.looseObject({
  locations: z.array(ReferenceLocationLocalSchema).optional(),
  references: z.array(ReferenceLocationLocalSchema).optional(),
  byFile: z.array(ReferencesByFileLocalSchema).optional(),
  totalReferences: z.number().optional(),
  totalFiles: z.number().optional(),
  pagination: LspPaginationSchema.optional(),
  outputPagination: LspPaginationSchema.optional(),
  hasMultipleFiles: z.boolean().optional(),
  hints: z.array(z.string()).optional(),
  error: z.string().optional(),
  errorType: z.string().optional(),
  errorCode: z.string().optional(),
  resolvedPath: z.string().optional(),
  cwd: z.string().optional(),
  searchRadius: z.number().optional(),
});

const LspCallHierarchyDataLocalSchema = z.looseObject({
  item: CallHierarchyItemLocalSchema.optional(),
  incomingCalls: z.array(IncomingCallLocalSchema).optional(),
  outgoingCalls: z.array(OutgoingCallLocalSchema).optional(),
  calls: z.array(z.unknown()).optional(),
  pagination: LspPaginationSchema.optional(),
  outputPagination: LspPaginationSchema.optional(),
  direction: z.enum(['incoming', 'outgoing']).optional(),
  depth: z.number().optional(),
  hints: z.array(z.string()).optional(),
  error: z.string().optional(),
  errorType: z.string().optional(),
  errorCode: z.string().optional(),
  resolvedPath: z.string().optional(),
  cwd: z.string().optional(),
  searchRadius: z.number().optional(),
});

export const LspFindReferencesOutputLocalSchema = z.object({
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
        data: LspFindReferencesDataLocalSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        status: z.literal('error'),
        data: ErrorDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        data: LspFindReferencesDataLocalSchema,
      }),
    ])
  ),
});

export const LspCallHierarchyOutputLocalSchema = z.object({
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
        data: LspCallHierarchyDataLocalSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        status: z.literal('error'),
        data: ErrorDataSchema,
      }),
      z.strictObject({
        id: z.string().min(1),
        data: LspCallHierarchyDataLocalSchema,
      }),
    ])
  ),
});
