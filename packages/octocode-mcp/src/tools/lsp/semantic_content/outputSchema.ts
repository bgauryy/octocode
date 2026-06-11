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

const DisplayRangeSchema = z.object({
  startLine: z.number(),
  endLine: z.number(),
});

// 0-based `range` is internal-only; locations emit line-prefixed content and
// 1-based displayRange, which is what agents chain on.
const LocationSchema = z.object({
  uri: z.string(),
  content: z.string().optional(),
  displayRange: DisplayRangeSchema.optional(),
  isDefinition: z.boolean().optional(),
});
const LocationRowSchema = z.string();

// range/position are 0-based internals derived from the same location as
// foundAtLine (1-based) — the envelope emits only the agent-facing facts.
const ResolvedSymbolSchema = z.object({
  name: z.string(),
  uri: z.string(),
  foundAtLine: z.number(),
  orderHint: z.number().optional(),
});

const LspSchema = z.object({
  serverAvailable: z.boolean().optional(),
  provider: z.string().optional(),
  source: z.string().optional(),
});

const EvidenceSchema = z.object({
  confidence: z.enum(['high', 'medium', 'low']),
  complete: z.boolean(),
  reason: z.string().optional(),
});

const PaginationSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  totalResults: z.number(),
  hasMore: z.boolean(),
  itemsPerPage: z.number(),
  nextPage: z.number().optional(),
});

const CompactSymbolSchema = z.object({
  name: z.string(),
  kind: z.string(),
  line: z.number(),
  character: z.number(),
  endLine: z.number(),
  childCount: z.number(),
  containerName: z.string().optional(),
});
const CompactSymbolRowSchema = z.string();

const CompactCallTargetSchema = z.object({
  name: z.string(),
  kind: z.string(),
  uri: z.string(),
  line: z.number(),
  endLine: z.number(),
  selectionLine: z.number().optional(),
});
const CompactCallTargetRowSchema = z.string();

const CompactCallSchema = z.object({
  direction: z.enum(['incoming', 'outgoing']),
  item: CompactCallTargetSchema,
  ranges: z.array(z.object({ line: z.number(), character: z.number() })),
  rangeCount: z.number(),
  rangeSampleCount: z.number(),
  contentPreview: z.string().optional(),
});
const CompactCallRowSchema = z.string();

const CompletenessSchema = z.object({
  complete: z.boolean(),
  truncatedByDepth: z.boolean(),
  cycleCount: z.number(),
  failedRequestCount: z.number(),
  dynamicCallsExcluded: z.literal(true),
  stdlibCallsExcluded: z.number().optional(),
});

const ReferencesByFileSchema = z.object({
  uri: z.string(),
  count: z.number(),
  firstLine: z.number(),
  firstCharacter: z.number(),
  lines: z.array(z.number()),
  hasDefinition: z.boolean().optional(),
});
const ReferencesByFileRowSchema = z.string();

const PayloadSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('definition'),
    locations: z.array(z.union([LocationSchema, LocationRowSchema])),
  }),
  z.object({
    kind: z.literal('typeDefinition'),
    locations: z.array(z.union([LocationSchema, LocationRowSchema])),
  }),
  z.object({
    kind: z.literal('implementation'),
    locations: z.array(z.union([LocationSchema, LocationRowSchema])),
  }),
  z.object({
    kind: z.literal('references'),
    // groupByFile=true emits byFile INSTEAD OF the flat locations list.
    locations: z.array(z.union([LocationSchema, LocationRowSchema])).optional(),
    byFile: z
      .array(z.union([ReferencesByFileSchema, ReferencesByFileRowSchema]))
      .optional(),
    totalReferences: z.number(),
    totalFiles: z.number(),
  }),
  ...(['callers', 'callees', 'callHierarchy'] as const).map(k =>
    z.object({
      kind: z.literal(k),
      root: z
        .union([CompactCallTargetSchema, CompactCallTargetRowSchema])
        .optional(),
      direction: z.enum(['incoming', 'outgoing', 'both']),
      calls: z.array(z.union([CompactCallSchema, CompactCallRowSchema])),
      // total count lives in pagination.totalResults; only the
      // incoming/outgoing split is unique information here.
      incomingCalls: z.number(),
      outgoingCalls: z.number(),
      completeness: CompletenessSchema,
    })
  ),
  z.object({
    kind: z.literal('hover'),
    markdown: z.string().optional(),
    text: z.string().optional(),
    range: RangeSchema.optional(),
  }),
  z.object({
    kind: z.literal('documentSymbols'),
    symbols: z.array(z.union([CompactSymbolSchema, CompactSymbolRowSchema])),
    totalSymbols: z.number().optional(),
    topLevelSymbols: z.number().optional(),
  }),
  z.object({ kind: z.literal('empty'), reason: z.string() }),
]);

const SemanticDataSchema = z.object({
  type: z.string(),
  uri: z.string(),
  format: z.enum(['structured', 'compact']).optional(),
  resolvedSymbol: ResolvedSymbolSchema.optional(),
  lsp: LspSchema,
  evidence: EvidenceSchema.optional(),
  payload: PayloadSchema,
  pagination: PaginationSchema.optional(),
  summary: z.record(z.string(), z.unknown()).optional(),
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
