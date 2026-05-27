import { z } from 'zod/v4';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';

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

export function describeShapeFields<
  Shape extends z.ZodRawShape,
  const Keys extends keyof Shape & string,
>(shape: Shape, descriptions: Record<Keys, string>): Pick<Shape, Keys> {
  const overrides = {} as Pick<Shape, Keys>;

  for (const [key, description] of Object.entries(descriptions)) {
    const shapeKey = key as Keys;
    const field = shape[shapeKey];
    if (field) {
      overrides[shapeKey] = (field as unknown as z.ZodTypeAny).describe(
        description as string
      ) as unknown as Shape[Keys];
    }
  }

  return overrides;
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

const viewStructureVerbosityField = createVerbosityField(
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

export const RipgrepQuerySchema = UpstreamRipgrepQuerySchema.extend({
  ...optionalMetaFields,
  ...describeShapeFields(UpstreamRipgrepQuerySchema.shape, {
    pattern: 'Pattern/regex (required)',
    mode: '"discovery" (file list, cheapest) | "paginated" (default) | "detailed" (full context, costliest)',
    fixedString: 'Literal match, no regex',
    smartCase: 'Case-insensitive unless pattern has uppercase',
    invertMatch: 'Return non-matching lines',
    type: 'Ripgrep language type ("ts", "js", "py", "go"...)',
    include: 'Include globs (["*.ts", "src/**"])',
    exclude: 'Exclude globs (["*.test.ts"])',
    excludeDir: 'Dir names to skip (["node_modules", "dist"])',
    noIgnore: 'Bypass .gitignore/.ignore',
    hidden: 'Include dotfiles',
    filesOnly: 'Filenames only, no content',
    filesWithoutMatch: 'Files NOT containing the pattern',
    count: 'Matching-line count per file',
    countMatches: 'Total match count per file (multi-match aware)',
    contextLines: 'Symmetric context around match',
    beforeContext: 'Lines before (overrides contextLines on that side)',
    afterContext: 'Lines after (overrides contextLines on that side)',
    maxMatchesPerFile: 'Cap matches per file',
    maxFiles: 'Cap total files scanned',
    multiline: 'Patterns may span newlines (slower)',
    multilineDotall: "In multiline, '.' matches newlines",
    binaryFiles: '"skip" | "text" | "binary"',
    includeStats: 'Include scan stats in response',
    encoding: 'Force encoding ("utf-8", "latin1"); else auto',
    sortReverse: 'Reverse sort order',
    noMessages: 'Suppress non-fatal errors',
    lineRegexp: 'Pattern must match entire line',
    passthru: 'Print every line; highlight matches',
    debug: 'Emit debug diagnostics',
    showFileLastModified: 'Include lastModified timestamps',
  }),
  matchContentLength: matchContentLengthField.describe(
    'Truncate each match line to N chars'
  ),
  verbosity: ripgrepVerbosityField,
  charLength: localCharLengthField,
  filesPerPage: relaxedPaginationLimitField
    .default(10)
    .describe('Files per page'),
  matchesPerPage: relaxedPaginationLimitField
    .default(10)
    .describe('Matches per file in response'),
  filePageNumber: relaxedPageNumberField.default(1).describe('1-indexed page'),
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

export const FindFilesQuerySchema = UpstreamFindFilesQuerySchema.extend({
  ...optionalMetaFields,
  ...describeShapeFields(UpstreamFindFilesQuerySchema.shape, {
    maxDepth: 'Max recursion depth',
    minDepth: 'Min depth from start',
    name: 'Glob name pattern (e.g. "*.js")',
    iname: 'Case-insensitive name glob',
    names: 'Glob array, OR-combined',
    pathPattern: 'Glob against full path, not basename',
    regex: 'Regex against name (or path with pathPattern semantics)',
    type: 'f (file) | d (dir) | l (symlink) | b | c | p | s',
    empty: 'true = match only empty files/dirs',
    modifiedWithin: 'Within duration ("7d", "2h", "30m")',
    modifiedBefore: 'Before duration ("30d")',
    accessedWithin: 'Accessed within ("7d")',
    sizeGreater: '">" size ("10M", "500k", "1G")',
    sizeLess: '"<" size ("1M")',
    permissions: 'Octal ("755") or symbolic ("u=rwx")',
    executable: 'true = executable by current user',
    readable: 'true = readable by current user',
    writable: 'true = writable by current user',
    excludeDir: 'Dir names to skip (e.g. ["node_modules", ".git"])',
    limit: 'Hard cap before paging',
    details: 'Include perms/size/dates',
    showFileLastModified: 'Include lastModified timestamps',
  }),
  charLength: localCharLengthField.describe('Max chars per payload page'),
  charOffset: UpstreamFindFilesQuerySchema.shape.charOffset.describe(
    'Char-level pagination offset'
  ),
  verbosity: findFilesVerbosityField,
  filesPerPage: relaxedPaginationLimitField
    .default(10)
    .describe('Results per page'),
  filePageNumber: relaxedPageNumberField.default(1).describe('1-indexed page'),
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

export const FetchContentQuerySchema = UpstreamFetchContentQuerySchema.extend({
  ...optionalMetaFields,
  ...describeShapeFields(UpstreamFetchContentQuerySchema.shape, {
    matchStringContextLines: 'Context lines around match',
    matchStringIsRegex: 'Treat matchString as regex',
  }),
  verbosity: fetchContentVerbosityField,
  charLength: localCharLengthField.describe('Max chars'),
  matchStringContextLines: matchStringContextLinesField
    .default(5)
    .describe('Context lines around match'),
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

export const ViewStructureQuerySchema = UpstreamViewStructureQuerySchema.extend(
  {
    ...optionalMetaFields,
    ...describeShapeFields(UpstreamViewStructureQuerySchema.shape, {
      details: 'Show perms/size/dates',
      humanReadable: 'Human sizes',
      entriesPerPage: 'Entries per page',
      depth: 'Recursion depth',
    }),
    charLength: localCharLengthField.describe('Max chars'),
    charOffset:
      UpstreamViewStructureQuerySchema.shape.charOffset.describe(
        'Pagination offset'
      ),
    verbosity: viewStructureVerbosityField,
    entriesPerPage: relaxedPaginationLimitField
      .default(20)
      .describe('Entries per page'),
    entryPageNumber: relaxedPageNumberField.default(1).describe('Page number'),
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
