import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import { validateFileContentExtractionMode } from './fileContentModeValidation.js';

export function describeField<T extends z.ZodTypeAny>(
  field: T,
  description: string
): T {
  return field.describe(description) as T;
}

export function withCoreSchemaDescriptions<
  T extends z.ZodObject<z.ZodRawShape>,
>(toolName: string, schema: T): T {
  const tool = completeMetadata.tools[toolName];
  const descriptions = {
    ...completeMetadata.baseSchema,
    ...(tool?.schema ?? {}),
  } as Record<string, unknown>;
  const describedShape = Object.fromEntries(
    Object.entries(schema.shape).map(([fieldName, fieldSchema]) => {
      const description = descriptions[fieldName];
      return [
        fieldName,
        typeof description === 'string'
          ? (fieldSchema as z.ZodTypeAny).describe(description)
          : fieldSchema,
      ];
    })
  ) as z.ZodRawShape;
  return schema.extend(describedShape) as unknown as T;
}

/**
 * Integer field that CLAMPS an out-of-range value into [min, max] instead of
 * rejecting it. A hard validation reject (MCP -32602 `too_big`/`too_small`)
 * wastes a metered tool call over a trivially-correctable magnitude — e.g.
 * `matchStringContextLines: 120` should just become 100 and proceed. Clamping
 * via `preprocess` keeps the inner `.min()/.max()`, so the published JSON
 * schema still advertises the bounds (no ±9e15 bloat). Non-integers and
 * non-numbers still reject downstream — those are genuine type errors, not
 * magnitudes.
 */
export function clampedInt(min: number, max: number) {
  return z.preprocess(
    v =>
      typeof v === 'number' && Number.isFinite(v)
        ? Math.min(Math.max(v, min), max)
        : v,
    z.number().int().min(min).max(max)
  );
}

export const LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH = 100_000;

const LOCAL_OVERLAY_MAX_CONTEXT_LINES = 100;

const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = 1_000;

// Caps the number of entries returned by find_files / view_structure before
// pagination. Prevents agents from driving unbounded fs walks via a single
// `limit: 1e9` call.
export const LOCAL_OVERLAY_MAX_LIMIT = 10_000;

// Caps recursion depth for view_structure and lsp_call_hierarchy.
export const LOCAL_OVERLAY_MAX_DEPTH = 20;

export const LOCAL_OVERLAY_MAX_LINE = 1_000_000_000;
export const LOCAL_OVERLAY_MAX_ORDER_HINT = 100_000;
export const LOCAL_OVERLAY_MAX_FS_DEPTH = 100;

/** 1-based line number / line hint (optional). */
export const lineNumberField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE).optional();

/** 1-based line hint that the LSP tools require (non-optional). */
export const requiredLineHintField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE);

/** 0-based occurrence index on the hinted line (optional). */
export const orderHintField = clampedInt(
  0,
  LOCAL_OVERLAY_MAX_ORDER_HINT
).optional();

/** Filesystem walk depth bound for localFindFiles min/maxDepth (optional). */
const fsDepthField = clampedInt(0, LOCAL_OVERLAY_MAX_FS_DEPTH).optional();

/**
 * Ripgrep pre-pagination cap (maxFiles / maxMatchesPerFile), optional.
 */
const ripgrepCapField = clampedInt(1, 100_000).optional();

const matchContentLengthField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH
)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 100000. ' +
      'Raise this when matches sit on very long lines (minified code, JSON blobs, generated SQL).'
  );

export const contextLinesField = clampedInt(0, LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe('Number of lines of context to show around each match. Max 100.');

export const relaxedPageNumberField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_PAGINATION_LIMIT
)
  .optional()
  .default(1);

/**
 * Page size constants — fixed per tool category for predictable, consistent
 * responses. Agents use `page` to navigate; item count per page is constant.
 *
 * DEFAULT_PAGE_SIZE (20): search tools (code, repos, PRs, packages), LSP tools.
 * STRUCTURE_PAGE_SIZE (100): navigation tools (view-structure, find-files)
 *   where entries are compact path strings (~30 chars each).
 */
export const DEFAULT_PAGE_SIZE = 20;
export const STRUCTURE_PAGE_SIZE = 100;

export const depthField = clampedInt(0, LOCAL_OVERLAY_MAX_DEPTH)
  .optional()
  .describe(
    `Recursion depth. Max ${LOCAL_OVERLAY_MAX_DEPTH}. For large trees, page the entries (page=N) or narrow the path rather than over-deepening.`
  );

/** Boolean detail switch added to every tool's per-query schema. */
export const verboseField = z
  .boolean()
  .optional()
  .describe(
    'Boolean detail switch shared by every tool query. false returns efficient research data; true includes extended metadata.'
  );

/**
 * Adds the optional verbose detail field to any upstream query type.
 */
export type WithVerbosity<T> = T & { verbose?: boolean };

/**
 * Cross-tool query-metadata shape.
 */
export type WithQueryMeta<T> = T & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};

/**
 * Composition of the two helpers above — the canonical shape that every
 * tool's query type now exposes.
 */
export type WithLocalOverlay<T> = WithVerbosity<WithQueryMeta<T>>;

/**
 * Returns the single verbose field. All tool schemas call this to add
 * the detail switch without duplicating the description.
 */
export function createVerbosityFields() {
  return {
    verbose: verboseField,
  } as const;
}

/**
 * Creates a bulk query schema for the given tool. Each bulk call accepts
 * 1–5 queries that run in parallel. Results are paginated at the item
 * level via the per-query `page` field — no char-based windowing.
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
        .max(maxQueries)
        .describe(
          `Array of queries for ${toolName}. Maximum is ${maxQueries} queries per call. ` +
            'Multiple queries run in parallel. Use the per-query `page` field to navigate through results.'
        ),
    })
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
              code: 'custom',
              message: `Duplicate query id "${q.id}" at index ${idx}`,
              path: ['queries', idx, 'id'],
            });
          }
          ids.add(q.id);
        }
      });
    });
}

/**
 * Optional research-metadata fields shared by every tool's per-query schema.
 */
export const optionalMetaFields = {
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
  ...createVerbosityFields(),
} as const;

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT)
  .optional()
  .describe(
    `Hard PRE-pagination cap: the maximum entries discovered before paging — ` +
      `distinct from the fixed page size (${DEFAULT_PAGE_SIZE} items/page for search tools, ` +
      `${STRUCTURE_PAGE_SIZE} for navigation tools). Use limit to bound a large fs walk. ` +
      `Max ${LOCAL_OVERLAY_MAX_LIMIT}.`
  );

const RIPGREP_HIDDEN_FIELDS = {
  matchesPerPage: true,
  filesPerPage: true,
  filePageNumber: true,
  smartCase: true,
  beforeContext: true,
  afterContext: true,
  binaryFiles: true,
  encoding: true,
  includeStats: true,
  noMessages: true,
  lineRegexp: true,
  passthru: true,
  debug: true,
  showFileLastModified: true,
  noUnicode: true,
  threads: true,
  mmap: true,
  followSymlinks: true,
} as const;

const RipgrepQueryBaseSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  UpstreamRipgrepQuerySchema.omit(RIPGREP_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    pattern: describeField(
      UpstreamRipgrepQuerySchema.shape.pattern,
      'Text or regex pattern to search for. Use fixedString=true for literal text and perlRegex=true only when regex features are required.'
    ),
    path: describeField(
      UpstreamRipgrepQuerySchema.shape.path,
      "File or directory to search. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    mode: describeField(
      UpstreamRipgrepQuerySchema.shape.mode,
      'Result shape: "paginated"/default for normal reading, "discovery" for cheap presence checks, "detailed" for expanded snippets.'
    ),
    matchContentLength: matchContentLengthField,
    invertMatch: UpstreamRipgrepQuerySchema.shape.invertMatch.describe(
      'Return lines/files NOT matching the pattern (-v). ' +
        'Combine with filesOnly to list files that lack a pattern entirely.'
    ),
    caseInsensitive: UpstreamRipgrepQuerySchema.shape.caseInsensitive.describe(
      'Force case-insensitive matching (-i). Overrides smartCase. ' +
        'Mutually exclusive with caseSensitive.'
    ),
    multiline: UpstreamRipgrepQuerySchema.shape.multiline.describe(
      'Enable cross-line matching (-U). Pattern can span multiple lines. ' +
        'Pair with perlRegex for named captures; pair with multilineDotall to let . match newlines.'
    ),
    multilineDotall: UpstreamRipgrepQuerySchema.shape.multilineDotall.describe(
      'Make . match newlines in multiline mode (--multiline-dotall). ' +
        'Requires multiline=true.'
    ),
    sort: z
      .enum(['path', 'modified', 'accessed', 'created'])
      .optional()
      .default('path')
      .describe(
        'Sort results by: path (default, deterministic), modified (most recently changed first), ' +
          'accessed, or created.'
      ),
    sortReverse: UpstreamRipgrepQuerySchema.shape.sortReverse.describe(
      'Reverse sort direction. Pair with sort (e.g. sort=modified + sortReverse=true for oldest first).'
    ),
    contextLines: contextLinesField.default(2),
    maxFiles: ripgrepCapField,
    maxMatchesPerFile: ripgrepCapField,
    // Pagination: fixed page size (DEFAULT_PAGE_SIZE files/page for search tools).
    // Files are the top-level atomic item; matches/file is the secondary axis.
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} files. Use page=2, page=3, … to walk through results.`
      ),
  })
);

export const RipgrepQuerySchema = RipgrepQueryBaseSchema.superRefine(
  (data, ctx) => {
    const d = data as {
      filesOnly?: boolean;
      filesWithoutMatch?: boolean;
      fixedString?: boolean;
      perlRegex?: boolean;
      caseSensitive?: boolean;
      caseInsensitive?: boolean;
      multiline?: boolean;
      multilineDotall?: boolean;
    };
    if (d.filesOnly === true && d.filesWithoutMatch === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`filesOnly` and `filesWithoutMatch` are mutually exclusive. Choose ONE: filesOnly=true for paths with matches, OR filesWithoutMatch=true for paths without matches.',
        path: ['filesWithoutMatch'],
      });
    }
    if (d.fixedString === true && d.perlRegex === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`fixedString` and `perlRegex` are mutually exclusive. fixedString treats the pattern as a literal string; perlRegex treats it as a Perl-compatible regex. Choose ONE.',
        path: ['perlRegex'],
      });
    }
    if (d.caseSensitive === true && d.caseInsensitive === true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`caseSensitive` and `caseInsensitive` are mutually exclusive. Choose ONE.',
        path: ['caseInsensitive'],
      });
    }
    if (d.multilineDotall === true && d.multiline !== true) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          '`multilineDotall` requires `multiline=true`. Set multiline=true to enable cross-line matching first.',
        path: ['multilineDotall'],
      });
    }
  }
);

export type RipgrepQuery = WithLocalOverlay<
  z.infer<typeof UpstreamRipgrepQuerySchema>
>;

export const BulkRipgrepQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQueryBaseSchema,
  { maxQueries: 5 }
);

export const FindFilesQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  UpstreamFindFilesQuerySchema.omit({
    filesPerPage: true,
    filePageNumber: true,
  }).extend({
    ...optionalMetaFields,
    path: describeField(
      UpstreamFindFilesQuerySchema.shape.path,
      "Directory root for metadata search. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    name: describeField(
      UpstreamFindFilesQuerySchema.shape.name,
      'Case-sensitive filename glob such as "*.ts".'
    ),
    iname: describeField(
      UpstreamFindFilesQuerySchema.shape.iname,
      'Case-insensitive filename glob, useful for README/readme or mixed-case filenames.'
    ),
    names: describeField(
      UpstreamFindFilesQuerySchema.shape.names,
      'Multiple filename globs OR-combined in one metadata search.'
    ),
    pathPattern: describeField(
      UpstreamFindFilesQuerySchema.shape.pathPattern,
      'Glob matched against the full path, useful for monorepo package roots or nested directory slices.'
    ),
    minDepth: fsDepthField,
    maxDepth: fsDepthField,
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} files. Use page=2, page=3, … to walk through results.`
      ),
    limit: limitField,
  })
);

export type FindFilesQuery = WithLocalOverlay<
  z.infer<typeof UpstreamFindFilesQuerySchema>
>;

export const BulkFindFilesSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQuerySchema,
  { maxQueries: 5 }
);

const FetchContentQueryBaseSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  UpstreamFetchContentQuerySchema.extend({
    ...optionalMetaFields,
    path: describeField(
      UpstreamFetchContentQuerySchema.shape.path,
      "File path to read. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
    ),
    fullContent: describeField(
      UpstreamFetchContentQuerySchema.shape.fullContent,
      'Read the whole file. Mutually exclusive with matchString and startLine/endLine.'
    ),
    matchString: describeField(
      UpstreamFetchContentQuerySchema.shape.matchString,
      'Anchor text or regex used to return matching slices with matchStringContextLines around each match.'
    ),
    startLine: describeField(
      lineNumberField,
      '1-based first line to include. Use with endLine; mutually exclusive with fullContent and matchString.'
    ),
    endLine: describeField(
      lineNumberField,
      '1-based last line to include. Use with startLine; mutually exclusive with fullContent and matchString.'
    ),
    matchStringContextLines: contextLinesField.default(5),
    page: relaxedPageNumberField.describe(
      '1-based page number for char-based pagination. When a matchString or full-file read is truncated, the response includes pagination.totalPages. Re-call with page=2, page=3, etc. to read subsequent chunks. Ignored when content fits in one page.'
    ),
  })
);

export const FetchContentQuerySchema = FetchContentQueryBaseSchema.superRefine(
  validateFileContentExtractionMode
);

export type FetchContentQuery = WithLocalOverlay<
  z.infer<typeof UpstreamFetchContentQuerySchema>
>;

export const BulkFetchContentQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FETCH_CONTENT,
  FetchContentQueryBaseSchema,
  { maxQueries: 5 }
);

const VIEW_STRUCTURE_HIDDEN_FIELDS = {
  extension: true,
  recursive: true,
  entriesPerPage: true,
  entryPageNumber: true,
} as const;

export const ViewStructureQuerySchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  UpstreamViewStructureQuerySchema.omit(VIEW_STRUCTURE_HIDDEN_FIELDS).extend({
    ...optionalMetaFields,
    path: describeField(
      UpstreamViewStructureQuerySchema.shape.path,
      "Directory to browse. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS). Start at the repo root with depth=1."
    ),
    page: relaxedPageNumberField
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} directory entries. Use page=2, page=3, … to walk through large directories.`
      ),
    limit: limitField,
    depth: depthField,
  })
);

export type ViewStructureQuery = WithLocalOverlay<
  z.infer<typeof UpstreamViewStructureQuerySchema>
>;

export const BulkViewStructureSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  ViewStructureQuerySchema,
  { maxQueries: 5 }
);
