import { z } from 'zod';
import { LspGetSemanticsQuerySchema as CoreLspGetSemanticsQuerySchema } from '@octocodeai/octocode-core/schemas';
import { ErrorDataSchema } from '@octocodeai/octocode-core/schemas/outputs';
import { LOCAL_MAX_DEPTH } from '../../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../../scheme/coreSchemas.js';
import { SEMANTIC_CONTENT_TYPES } from '../shared/semanticTypes.js';

const requiredLineHintField = clampedInt(1, 1_000_000_000);
const orderHintField = clampedInt(0, 100_000).optional();

const SEMANTIC_OUTPUT_FORMATS = ['structured', 'compact'] as const;

const queryOverrides = {
  type: z.enum(SEMANTIC_CONTENT_TYPES).default('definition'),
  symbolName: z.string().min(1).optional(),
  lineHint: requiredLineHintField.optional(),
  orderHint: orderHintField,
  depth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  includeDeclaration: z.boolean().optional().default(true),
  page: relaxedPageNumberField,
  itemsPerPage: clampedInt(1, 100).optional(),
  contextLines: clampedInt(0, 100).optional(),
  format: z.enum(SEMANTIC_OUTPUT_FORMATS).optional().default('structured'),
} as const;

const SemanticContentQueryShape = createQueryShapeSchema(
  CoreLspGetSemanticsQuerySchema,
  queryOverrides
);

export const LspGetSemanticsQueryDisplaySchema = describeQuerySchema(
  CoreLspGetSemanticsQuerySchema,
  queryOverrides
);

export const LspGetSemanticsQuerySchema = LspGetSemanticsQueryDisplaySchema;

export const BulkLspGetSemanticsQuerySchema = createRelaxedBulkQuerySchema(
  SemanticContentQueryShape,
  { maxQueries: 5 }
);

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

const LocationSchema = z.object({
  uri: z.string(),
  content: z.string().optional(),
  displayRange: DisplayRangeSchema.optional(),
  isDefinition: z.boolean().optional(),
});
const LocationRowSchema = z.string();

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

const EmptyCategorySchema = z.enum([
  'serverUnavailable',
  'unsupportedOperation',
  'symbolNotFound',
  'anchorFailed',
  'noLocations',
  'noReferences',
  'noHover',
  'noCalls',
]);

const EmptyStateSchema = z.object({
  category: EmptyCategorySchema,
  reason: z.string(),
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
    locations: z.array(z.union([LocationSchema, LocationRowSchema])).optional(),
    byFile: z
      .array(z.union([ReferencesByFileSchema, ReferencesByFileRowSchema]))
      .optional(),
    totalReferences: z.number(),
    totalFiles: z.number(),
    empty: EmptyStateSchema.optional(),
  }),
  ...(['callers', 'callees', 'callHierarchy'] as const).map(k =>
    z.object({
      kind: z.literal(k),
      root: z
        .union([CompactCallTargetSchema, CompactCallTargetRowSchema])
        .optional(),
      direction: z.enum(['incoming', 'outgoing', 'both']),
      calls: z.array(z.union([CompactCallSchema, CompactCallRowSchema])),
      incomingCalls: z.number(),
      outgoingCalls: z.number(),
      completeness: CompletenessSchema,
      empty: EmptyStateSchema.optional(),
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
    empty: EmptyStateSchema.optional(),
  }),
  z.object({
    kind: z.literal('empty'),
    category: EmptyCategorySchema,
    reason: z.string(),
  }),
]);

const SemanticDataSchema = z.object({
  type: z.string(),
  uri: z.string(),
  format: z.enum(['structured', 'compact']).optional(),
  resolvedSymbol: ResolvedSymbolSchema.optional(),
  lsp: LspSchema,
  payload: PayloadSchema,
  pagination: PaginationSchema.optional(),
  summary: z.record(z.string(), z.unknown()).optional(),
  warnings: z.array(z.string()).optional(),
  hints: z.array(z.string()).optional(),
});

export const LspGetSemanticsOutputSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  hints: z.array(z.string()).optional(),
  results: z.array(
    z.union([
      z.object({
        id: z.string().min(1),
        status: z.literal('empty'),
        data: SemanticDataSchema,
      }),
      z.object({
        id: z.string().min(1),
        status: z.literal('error'),
        data: ErrorDataSchema,
      }),
      z.object({
        id: z.string().min(1),
        data: SemanticDataSchema,
      }),
    ])
  ),
});
