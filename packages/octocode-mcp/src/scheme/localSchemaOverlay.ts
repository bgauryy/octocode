import { z } from 'zod/v4';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';

export const LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH = 500_000;

export const LOCAL_OVERLAY_MAX_CHAR_LENGTH = 500_000;

export const LOCAL_OVERLAY_MAX_CONTEXT_LINES = 1000;

export const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = 10_000;

export const matchContentLengthField = z
  .number()
  .int()
  .min(1)
  .max(LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 500000. ' +
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
    'Character budget for output pagination of this query. Unified at 500000 across local tools. ' +
      'Pair with charOffset for explicit pagination instead of truncating responses.'
  );

export const matchStringContextLinesField = z
  .number()
  .int()
  .min(0)
  .max(LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe(
    'Number of lines of context to show around each match. Default 5, recommended max 100.'
  );

export const contextLinesField = z
  .number()
  .int()
  .min(0)
  .max(LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe(
    'Number of lines of context to show around each match. Default 5, recommended max 100.'
  );

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

export const VERBOSITY_VALUES = ['compact', 'verbose', 'ultra'] as const;
export type Verbosity = (typeof VERBOSITY_VALUES)[number];

export const verbosityField = z
  .enum(VERBOSITY_VALUES)
  .optional()
  .describe(
    'Choose response size. Less tokens per call leaves more budget for follow-up. ' +
      "'compact' is the default and returns actionable detail. " +
      "'ultra' returns lossy counts/summaries for cheap broad probes. " +
      "'verbose' currently equals compact; skip it unless tool docs say otherwise. " +
      "Drill-back: re-call with 'compact' for paths, lines, snippets, or entries."
  );

export function createVerbosityField(
  toolDetail: string,
  ultraDetail: string,
  drillBack: string
) {
  return z
    .enum(VERBOSITY_VALUES)
    .optional()
    .describe(
      `Choose response size. compact (default): ${toolDetail}; use for normal work and follow-up line hints. ` +
        `ultra: ${ultraDetail}; use first for broad/large probes when counts or top locations are enough. ` +
        'verbose: currently same as compact; skip it unless future docs say it adds detail. ' +
        `Drill-back from ultra: ${drillBack}.`
    );
}

const ripgrepVerbosityField = createVerbosityField(
  'files[] with path:line matches, snippets, match counts, search engine, and pagination',
  'match/file counts plus the top path:line; files[] and match snippets are dropped',
  're-call with verbosity:"compact" or scope path/include to the top path'
);

const findFilesVerbosityField = createVerbosityField(
  'files[] with paths, type, size, permissions, timestamps, and pagination',
  'file/dir counts plus the newest path; files[] is dropped',
  're-call with verbosity:"compact" or narrow name/type/time filters'
);

const fetchContentVerbosityField = createVerbosityField(
  'content for the requested file/slice plus line ranges, matchRanges, partial flag, and pagination',
  'line/token estimates and ranges with content set to empty',
  're-call with verbosity:"compact", matchString, or a startLine/endLine range'
);

export const viewStructureVerbosityField = createVerbosityField(
  'entries[] with names, types, size/modified metadata, summary, and pagination',
  'entry/file/dir counts and summary; entries[] is dropped',
  're-call with verbosity:"compact" and entryPageNumber/entriesPerPage'
);

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
        .preprocess(val => {
          if (Array.isArray(val)) {
            // Filter out non-object items (like hallucinated string hints)
            return val.filter(
              item => typeof item === 'object' && item !== null
            );
          }
          return val;
        }, z.array(querySchema).min(1))
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
  id: z.string().optional(),
  researchGoal: z.string().optional(),
  reasoning: z.string().optional(),
} as const;

export const RipgrepQuerySchema = UpstreamRipgrepQuerySchema.extend({
  ...optionalMetaFields,
  matchContentLength: matchContentLengthField,
  verbosity: ripgrepVerbosityField,
  charLength: localCharLengthField,
  filesPerPage: relaxedPaginationLimitField.default(10),
  matchesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
});

export type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;

export const BulkRipgrepQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQuerySchema,
  { maxQueries: 5 }
);

export const FindFilesQuerySchema = UpstreamFindFilesQuerySchema.extend({
  ...optionalMetaFields,
  charLength: localCharLengthField,
  verbosity: findFilesVerbosityField,
  filesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
});

export type FindFilesQuery = z.infer<typeof FindFilesQuerySchema>;

export const BulkFindFilesSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQuerySchema,
  { maxQueries: 5 }
);

export const FetchContentQuerySchema = UpstreamFetchContentQuerySchema.extend({
  ...optionalMetaFields,
  verbosity: fetchContentVerbosityField,
  charLength: localCharLengthField,
  matchStringContextLines: matchStringContextLinesField.default(5),
});

export type FetchContentQuery = z.infer<typeof FetchContentQuerySchema>;

export const BulkFetchContentQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQuerySchema,
  { maxQueries: 5 }
);

export const ViewStructureQuerySchema = UpstreamViewStructureQuerySchema.extend(
  {
    ...optionalMetaFields,
    charLength: localCharLengthField,
    verbosity: viewStructureVerbosityField,
    entriesPerPage: relaxedPaginationLimitField.default(20),
    entryPageNumber: relaxedPageNumberField.default(1),
  }
);

export type ViewStructureQuery = z.infer<typeof ViewStructureQuerySchema>;

export const BulkViewStructureSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  ViewStructureQuerySchema,
  { maxQueries: 5 }
);
