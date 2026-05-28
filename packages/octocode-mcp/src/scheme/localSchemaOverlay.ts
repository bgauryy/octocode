import { z } from 'zod/v4';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
  VERBOSITY_VALUES,
  type Verbosity,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';

// Re-export the canonical enum + type so consumers in this package don't have
// to import from @octocodeai/octocode-core directly.
export { VERBOSITY_VALUES };
export type { Verbosity };

export const LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH = 100_000;

export const LOCAL_OVERLAY_MAX_CHAR_LENGTH = 100_000;

const LOCAL_OVERLAY_MAX_CONTEXT_LINES = 100;

const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = 1_000;

const matchContentLengthField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 100000. ' +
      'Raise this when matches sit on very long lines (minified code, JSON blobs, generated SQL). ' +
      'Total output size is still bounded by charLength / responseCharLength budgets — ' +
      'prefer paginating via filePageNumber/matchesPerPage over truncating a single match.'
  );

export const localCharLengthField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_CHAR_LENGTH)
  .optional()
  .describe(
    'Character budget for output pagination of this query. Unified at 100000 across local tools. ' +
      'Pair with charOffset for explicit pagination instead of truncating responses.'
  );

export const matchStringContextLinesField = z
  .number()
  .int()
  .min(0)
  .max(LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe('Number of lines of context to show around each match. Max 100.');

export const contextLinesField = z
  .number()
  .int()
  .min(0)
  .max(LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe('Number of lines of context to show around each match. Max 100.');

export const relaxedPaginationLimitField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_PAGINATION_LIMIT)
  .optional();

export const relaxedPageNumberField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_PAGINATION_LIMIT)
  .optional();

// All field-description text lives upstream in
// octocode-core/src/resources/global.ts `baseSchema.verbosity`. Overlay
// supplies only the Zod enum so bulk validation accepts the field.
export const verbosityField = z.enum(VERBOSITY_VALUES).optional();

/**
 * Per-tool verbosity field. Description text comes from upstream
 * `baseSchema.verbosity` — do not redescribe here.
 */
export function createVerbosityField() {
  return z.enum(VERBOSITY_VALUES).optional();
}

// All tools share the same Zod field; description text comes from upstream
// baseSchema.verbosity. Tool-specific guidance for verbosity goes into the
// tool's own <gotchas> in octocode-core/src/resources/tools/*.ts.
const ripgrepVerbosityField = createVerbosityField();
const findFilesVerbosityField = createVerbosityField();
const fetchContentVerbosityField = createVerbosityField();
const viewStructureVerbosityField = createVerbosityField();

/**
 * Creates a bulk query schema that is less strict than the upstream one.
 * It removes the hard limit on the number of queries (moving it to the description)
 * and pre-processes the queries array to filter out non-object items (like strings).
 */
export function createRelaxedBulkQuerySchema(
  toolName: string,
  querySchema: z.ZodTypeAny,
  options: { maxQueries?: number } = {}
) {
  const { maxQueries = 5 } = options;
  return z
    .object({
      queries: z
        .array(querySchema)
        .min(1)
        .describe(
          `Array of queries for ${toolName}. Recommended maximum is ${maxQueries} queries per call. ` +
            'Multiple queries run in parallel. If many are provided, results may be truncated to fit token limits.'
        ),
      responseCharOffset: z
        .number()
        .int()
        .min(0)
        .optional()
        .describe(
          'Optional character offset for the aggregated response. Use for paginating very large bulk results.'
        ),
      responseCharLength: z
        .number()
        .int()
        .min(1)
        .optional()
        .describe(
          'Optional character limit for the aggregated response. Use to control token usage.'
        ),
      format: z
        .enum(['tsv', 'json'])
        .default('tsv')
        .describe(
          'Output format. "tsv" (default) emits a tab-delimited rows view ' +
            'optimized for token efficiency. "json" preserves the structured ' +
            'nested response when callers need every field.'
        ),
    })
    .strip()
    .superRefine((data, ctx) => {
      const ids = new Set<string>();
      data.queries.forEach((q: unknown, idx) => {
        if (
          q &&
          typeof q === 'object' &&
          'id' in q &&
          typeof q.id === 'string'
        ) {
          if (ids.has(q.id)) {
            ctx.addIssue({
              code: z.ZodIssueCode.custom,
              message: `Duplicate query id "${q.id}" at index ${idx}`,
              path: ['queries', idx, 'id'],
            });
          }
          ids.add(q.id);
        }
      });
    });
}

const optionalMetaFields = {
  id: z.string().optional().describe('Stable query identifier.'),
  mainResearchGoal: z
    .string()
    .optional()
    .describe('Overall research objective shared by related queries.'),
  researchGoal: z
    .string()
    .optional()
    .describe('Specific goal this query is trying to answer.'),
  reasoning: z
    .string()
    .optional()
    .describe('Why this query helps achieve the research goal.'),
} as const;

// Field descriptions are upstream (localSearchCode.ts). Overlay supplies only
// the verbosity field, the relaxed numeric ranges, and pagination defaults.
export const RipgrepQuerySchema = UpstreamRipgrepQuerySchema.extend({
  ...optionalMetaFields,
  matchContentLength: matchContentLengthField,
  verbosity: ripgrepVerbosityField,
  charLength: localCharLengthField,
  filesPerPage: relaxedPaginationLimitField.default(10),
  matchesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
});

export type RipgrepQuery = z.infer<typeof UpstreamRipgrepQuerySchema> & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
  verbosity?: Verbosity;
};

export const BulkRipgrepQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQuerySchema,
  { maxQueries: 5 }
);

// Field descriptions are upstream (localFindFiles.ts). Overlay supplies only
// the verbosity field, the relaxed numeric ranges, and pagination defaults.
export const FindFilesQuerySchema = UpstreamFindFilesQuerySchema.extend({
  ...optionalMetaFields,
  charLength: localCharLengthField,
  verbosity: findFilesVerbosityField,
  filesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
});

export type FindFilesQuery = z.infer<typeof UpstreamFindFilesQuerySchema> & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
  verbosity?: Verbosity;
};

export const BulkFindFilesSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQuerySchema,
  { maxQueries: 5 }
);

// Field descriptions are upstream (localGetFileContent.ts). Overlay supplies
// only the verbosity field, char-budget range, and matchStringContextLines default.
export const FetchContentQuerySchema = UpstreamFetchContentQuerySchema.extend({
  ...optionalMetaFields,
  verbosity: fetchContentVerbosityField,
  charLength: localCharLengthField,
  matchStringContextLines: matchStringContextLinesField.default(5),
});

export type FetchContentQuery = z.infer<
  typeof UpstreamFetchContentQuerySchema
> & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
  verbosity?: Verbosity;
};

export const BulkFetchContentQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQuerySchema,
  { maxQueries: 5 }
);

// Field descriptions are upstream (localViewStructure.ts). Overlay supplies
// only the verbosity field, char-budget range, and pagination defaults.
export const ViewStructureQuerySchema = UpstreamViewStructureQuerySchema.extend(
  {
    ...optionalMetaFields,
    charLength: localCharLengthField,
    verbosity: viewStructureVerbosityField,
    entriesPerPage: relaxedPaginationLimitField.default(20),
    entryPageNumber: relaxedPageNumberField.default(1),
  }
);

export type ViewStructureQuery = z.infer<
  typeof UpstreamViewStructureQuerySchema
> & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
  verbosity?: Verbosity;
};

export const BulkViewStructureSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  ViewStructureQuerySchema,
  { maxQueries: 5 }
);
