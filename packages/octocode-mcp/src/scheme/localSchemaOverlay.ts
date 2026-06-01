import { z } from 'zod/v4';
import {
  RipgrepQuerySchema as UpstreamRipgrepQuerySchema,
  FindFilesQuerySchema as UpstreamFindFilesQuerySchema,
  ViewStructureQuerySchema as UpstreamViewStructureQuerySchema,
  FetchContentQuerySchema as UpstreamFetchContentQuerySchema,
} from '@octocodeai/octocode-core/schemas';
import { VERBOSITY_VALUES } from '@octocodeai/octocode-core/types';
import type { Verbosity } from '@octocodeai/octocode-core/types';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import { validateFileContentExtractionMode } from './fileContentModeValidation.js';

// Re-export the canonical enum + type so consumers in this package don't have
// to import from @octocodeai/octocode-core directly.
export { VERBOSITY_VALUES };
export type { Verbosity };

export function describeField<T extends z.ZodTypeAny>(
  field: T,
  description: string
): T {
  return field.describe(description) as T;
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

export const LOCAL_OVERLAY_MAX_CHAR_LENGTH = 100_000;

const LOCAL_OVERLAY_MAX_CONTEXT_LINES = 100;

const LOCAL_OVERLAY_MAX_PAGINATION_LIMIT = 1_000;

// Caps the aggregated response char offset. Agents that want to scan deeper
// must paginate via individual queries — there is no legitimate use case for
// skipping more than this many characters of a single bulk response.
export const LOCAL_OVERLAY_MAX_RESPONSE_CHAR_OFFSET = 10_000_000;

// Caps the number of entries returned by find_files / view_structure before
// pagination. Prevents agents from driving unbounded fs walks via a single
// `limit: 1e9` call.
export const LOCAL_OVERLAY_MAX_LIMIT = 10_000;

// Caps recursion depth for view_structure and lsp_call_hierarchy. Twenty is
// already deeper than any realistic source tree or call graph; deeper scans
// should paginate via pageNumber instead.
export const LOCAL_OVERLAY_MAX_DEPTH = 20;

// Caps for line numbers, occurrence indices, filesystem walk depth, and the
// ripgrep pre-pagination caps. These exist mainly to give the upstream
// `z.number().int()` fields real bounds: without an explicit `.min()/.max()`,
// zod serializes them as ±9007199254740991 (the JS safe-integer range) into
// every published inputSchema — token bloat plus a validation gap (negatives
// and absurd values pass). Generous but finite.
export const LOCAL_OVERLAY_MAX_LINE = 1_000_000_000;
export const LOCAL_OVERLAY_MAX_ORDER_HINT = 100_000;
export const LOCAL_OVERLAY_MAX_FS_DEPTH = 100;

/** 1-based line number / line hint (optional). */
export const lineNumberField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE).optional();

/** 1-based line hint that the LSP tools require (non-optional). */
export const requiredLineHintField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE);

/** Non-negative character offset for output pagination (optional). */
export const charOffsetField = clampedInt(
  0,
  LOCAL_OVERLAY_MAX_RESPONSE_CHAR_OFFSET
).optional();

/** 0-based occurrence index on the hinted line (optional). */
export const orderHintField = clampedInt(
  0,
  LOCAL_OVERLAY_MAX_ORDER_HINT
).optional();

/** Filesystem walk depth bound for localFindFiles min/maxDepth (optional). */
const fsDepthField = clampedInt(0, LOCAL_OVERLAY_MAX_FS_DEPTH).optional();

/** Ripgrep pre-pagination cap (maxFiles / maxMatchesPerFile), optional. */
const ripgrepCapField = clampedInt(1, LOCAL_OVERLAY_MAX_LINE).optional();

const matchContentLengthField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_MATCH_CONTENT_LENGTH
)
  .optional()
  .default(200)
  .describe(
    'Maximum characters per individual match snippet. Default 200, max 100000. ' +
      'Raise this when matches sit on very long lines (minified code, JSON blobs, generated SQL). ' +
      'Total output size is still bounded by charLength / responseCharLength budgets — ' +
      'prefer paginating via filePageNumber/matchesPerPage over truncating a single match.'
  );

export const localCharLengthField = clampedInt(1, LOCAL_OVERLAY_MAX_CHAR_LENGTH)
  .optional()
  .describe(
    'Character budget for output pagination of this query. Unified at 100000 across local tools. ' +
      'Pair with charOffset for explicit pagination instead of truncating responses.'
  );

export const contextLinesField = clampedInt(0, LOCAL_OVERLAY_MAX_CONTEXT_LINES)
  .optional()
  .describe('Number of lines of context to show around each match. Max 100.');

export const relaxedPaginationLimitField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_PAGINATION_LIMIT
).optional();

export const relaxedPageNumberField = clampedInt(
  1,
  LOCAL_OVERLAY_MAX_PAGINATION_LIMIT
).optional();

const limitField = clampedInt(1, LOCAL_OVERLAY_MAX_LIMIT)
  .optional()
  .describe(
    `Maximum entries returned before pagination. Max ${LOCAL_OVERLAY_MAX_LIMIT}.`
  );

export const depthField = clampedInt(0, LOCAL_OVERLAY_MAX_DEPTH)
  .optional()
  .describe(
    `Recursion depth. Max ${LOCAL_OVERLAY_MAX_DEPTH}. Paginate via pageNumber for deeper scans.`
  );

// All field-description text lives upstream in
// octocode-core/src/resources/global.ts `baseSchema.verbosity`. Overlay
// supplies only the Zod enum so bulk validation accepts the field.
export const verbosityField = z.enum(VERBOSITY_VALUES).optional();

/**
 * Generic helper: adds the optional `verbosity` field to any upstream query
 * type. Lets per-tool query types avoid redeclaring the same `verbosity?:
 * Verbosity` shape.
 */
export type WithVerbosity<T> = T & { verbosity?: Verbosity };

/**
 * Cross-tool query-metadata shape. Mirrors `optionalMetaFields` so per-tool
 * query types can compose it without redeclaring every optional field.
 */
export type WithQueryMeta<T> = T & {
  id?: string;
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
};

/**
 * Composition of the two helpers above — the canonical shape that every
 * tool's query type now exposes. Replaces the hand-written intersections
 * scattered across this file.
 */
export type WithLocalOverlay<T> = WithVerbosity<WithQueryMeta<T>>;

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
// Call createVerbosityField() inline at each .extend() site below.

/**
 * Creates a bulk query schema that is less strict than the upstream one.
 * It keeps the shared bulk envelope consistent across local, remote, and LSP
 * overlays while enforcing the public 1-5 query contract.
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
            'Multiple queries run in parallel. Large results are paginated — page ' +
            'through them with responseCharOffset/responseCharLength to fetch more.'
        ),
      responseCharOffset: z
        .number()
        .int()
        .min(0)
        .max(LOCAL_OVERLAY_MAX_RESPONSE_CHAR_OFFSET)
        .optional()
        .describe(
          'Optional character offset for the aggregated response. Use for paginating very large bulk results. ' +
            `Max ${LOCAL_OVERLAY_MAX_RESPONSE_CHAR_OFFSET} — paginate via individual queries for deeper scans.`
        ),
      responseCharLength: z
        .number()
        .int()
        .min(1)
        .max(LOCAL_OVERLAY_MAX_CHAR_LENGTH)
        .optional()
        .describe(
          `Optional character limit for the aggregated response. Use to control token usage. Max ${LOCAL_OVERLAY_MAX_CHAR_LENGTH}.`
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

/**
 * Optional research-metadata fields shared by every tool's per-query schema.
 * Exported so LSP / remote overlays reuse the same definitions and
 * descriptions instead of duplicating them.
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
} as const;

// Field descriptions are upstream (localSearchCode.ts). Overlay supplies only
// the verbosity field, the relaxed numeric ranges, and pagination defaults.
// The superRefine mirrors the runtime mutex checks in
// octocode-core/src/schemas/runtime.ts at the schema layer, so conflicting
// inputs are rejected with a structured Zod error before the executor
// runs. The runtime check stays in place as defense in depth for callers
// that bypass the MCP overlay.
// Agent-facing surface reduction. The upstream ripgrep schema carries every
// `rg` flag (~34 options); most are performance knobs, encoding/binary edge
// cases, or diagnostics that an LLM should never tune and that only bloat the
// tool schema. We hide them from the MCP surface so the agent sees a focused,
// high-signal set aligned with ripgrep code-search best practices. They remain
// fully functional upstream (CLI, direct command-builder use) — only omitted
// from the generated MCP inputSchema. Kept: pattern/path, mode, the pattern
// modifiers that change *what* matches (fixedString, perlRegex, wholeWord,
// caseSensitive), scoping (type/include/exclude/excludeDir/hidden/noIgnore),
// output modes (filesOnly/filesWithoutMatch/count/countMatches), symmetric
// contextLines, and the caps/pagination knobs.
const RIPGREP_HIDDEN_FIELDS = {
  smartCase: true, // default behavior — no need to expose; caseSensitive overrides
  caseInsensitive: true, // smartCase covers it; rare to force on an uppercase pattern
  invertMatch: true, // niche ("lines NOT matching")
  beforeContext: true, // asymmetric context — contextLines covers the common case
  afterContext: true,
  multiline: true, // line-based search covers ~all code lookups
  multilineDotall: true,
  binaryFiles: true, // default (skip binaries) is right; agents don't grep binaries
  encoding: true, // auto-detection is reliable
  includeStats: true, // diagnostic
  noMessages: true, // agents should SEE errors, not suppress them
  lineRegexp: true, // niche (whole-line match)
  passthru: true, // prints all lines — conflicts with structured output
  debug: true, // diagnostic
  showFileLastModified: true, // metadata → localFindFiles
  sort: true, // result ordering → default path order; time-sort via localFindFiles
  sortReverse: true,
  noUnicode: true, // perf/encoding edge
  threads: true, // perf knob — never agent-relevant
  mmap: true, // perf knob
  followSymlinks: true, // niche
} as const;

// Base (relaxed) per-query shape — NO mutex superRefine. The bulk envelope
// wraps THIS so a single malformed query never rejects the whole batch at MCP
// input-validation time; the executor re-validates each query against the
// strict schema below and emits a per-query error instead. Honors the bulk
// contract: "one errored entry must not block the others."
const RipgrepQueryBaseSchema = UpstreamRipgrepQuerySchema.omit(
  RIPGREP_HIDDEN_FIELDS
).extend({
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
    'Result shape: paginated/default for normal reading, discovery for cheap presence checks, detailed for expanded snippets.'
  ),
  matchContentLength: matchContentLengthField,
  verbosity: createVerbosityField(),
  charLength: localCharLengthField,
  filesPerPage: relaxedPaginationLimitField.default(10),
  matchesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
  // Symmetric context window, clamped so one query can't request an absurd
  // span. Asymmetric before/after is hidden — contextLines covers the need.
  contextLines: contextLinesField,
  // Pre-pagination hard caps — bounded so the published schema doesn't expose
  // the ±MAX_SAFE_INTEGER range and absurd values are rejected up front.
  maxFiles: ripgrepCapField,
  maxMatchesPerFile: ripgrepCapField,
});

// Strict per-query schema (base + mutex). The executor `safeParse`s each query
// against this and returns a per-query error on a mutex violation.
export const RipgrepQuerySchema = RipgrepQueryBaseSchema.superRefine(
  (data, ctx) => {
    const d = data as {
      filesOnly?: boolean;
      filesWithoutMatch?: boolean;
      fixedString?: boolean;
      perlRegex?: boolean;
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
  }
);

// DELIBERATE SUPERSET: inferred from the UPSTREAM schema, so this type still
// includes the fields hidden by `RIPGREP_HIDDEN_FIELDS` (threads, multiline,
// encoding, …). That is intentional — the MCP agent surface omits them, but the
// command builder and CLI still accept and act on them, and inferring from
// upstream keeps those code paths type-checking. The MCP overlay strips any
// hidden field at validation, so the runtime never sees one from an agent; the
// type is wider than the MCP surface on purpose.
export type RipgrepQuery = WithLocalOverlay<
  z.infer<typeof UpstreamRipgrepQuerySchema>
>;

export const BulkRipgrepQuerySchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_RIPGREP,
  RipgrepQueryBaseSchema,
  { maxQueries: 5 }
);

// Field descriptions are upstream (localFindFiles.ts). Overlay supplies only
// the verbosity field, the relaxed numeric ranges, and pagination defaults.
export const FindFilesQuerySchema = UpstreamFindFilesQuerySchema.extend({
  ...optionalMetaFields,
  path: describeField(
    UpstreamFindFilesQuerySchema.shape.path,
    "Directory root for metadata search. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS)."
  ),
  name: describeField(
    UpstreamFindFilesQuerySchema.shape.name,
    'Case-sensitive filename glob such as "*.ts". Use iname for case-insensitive matching or names for multiple globs.'
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
  charLength: localCharLengthField,
  charOffset: charOffsetField,
  minDepth: fsDepthField,
  maxDepth: fsDepthField,
  verbosity: createVerbosityField(),
  filesPerPage: relaxedPaginationLimitField.default(10),
  filePageNumber: relaxedPageNumberField.default(1),
  limit: limitField,
});

export type FindFilesQuery = WithLocalOverlay<
  z.infer<typeof UpstreamFindFilesQuerySchema>
>;

export const BulkFindFilesSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_FIND_FILES,
  FindFilesQuerySchema,
  { maxQueries: 5 }
);

// Field descriptions are upstream (localGetFileContent.ts). Overlay supplies
// only the verbosity field, char-budget range, and matchStringContextLines default.
// The superRefine enforces the same three extraction-mode mutex as
// githubGetFileContent: fullContent, matchString, and startLine/endLine are
// mutually exclusive ways to select content.
// Base (relaxed) per-query shape — NO extraction-mode mutex. The bulk envelope
// wraps THIS so a malformed query doesn't reject the whole batch; the executor
// re-validates each query against the strict schema below (per-query error).
const FetchContentQueryBaseSchema = UpstreamFetchContentQuerySchema.extend({
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
  verbosity: createVerbosityField(),
  charLength: localCharLengthField,
  charOffset: charOffsetField,
  matchStringContextLines: contextLinesField.default(5),
});

// Strict per-query schema (base + mutex). The executor `safeParse`s against
// this and emits a per-query error on a mutex violation.
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

// Field descriptions are upstream (localViewStructure.ts). Overlay supplies
// only the verbosity field, char-budget range, and pagination defaults.
//
// Overlap removal — two pairs in the upstream schema do the same job two ways:
//   • `extension` (singular) ⊂ `extensions` (array). Keep the array; hide the
//     singular so there is one obvious way to filter by extension.
//   • `recursive` (unbounded) vs `depth` (bounded). Keep `depth` (max 20);
//     hide `recursive` — "deep" is `depth: 20`, and unbounded fs walks are the
//     exact footgun the depth cap exists to prevent.
// Both stay functional upstream; only hidden from the MCP surface.
const VIEW_STRUCTURE_HIDDEN_FIELDS = {
  extension: true,
  recursive: true,
} as const;

export const ViewStructureQuerySchema = UpstreamViewStructureQuerySchema.omit(
  VIEW_STRUCTURE_HIDDEN_FIELDS
).extend({
  ...optionalMetaFields,
  path: describeField(
    UpstreamViewStructureQuerySchema.shape.path,
    "Directory to browse. Relative paths resolve against the server's working directory; absolute paths must be within an allowed root (home directory or ALLOWED_PATHS). Start at the repo root with depth=1."
  ),
  charLength: localCharLengthField,
  charOffset: charOffsetField,
  verbosity: createVerbosityField(),
  entriesPerPage: relaxedPaginationLimitField
    .describe('Number of directory entries per page before char pagination.')
    .default(20),
  entryPageNumber: relaxedPageNumberField.default(1),
  limit: limitField,
  depth: depthField,
});

// DELIBERATE SUPERSET (same rationale as RipgrepQuery): inferred from upstream,
// so it still carries the fields hidden by `VIEW_STRUCTURE_HIDDEN_FIELDS`
// (`extension`, `recursive`). The handler reads `query.recursive`/`extension`
// for CLI/direct callers; the MCP overlay strips them so an agent can't pass
// them. The type is intentionally wider than the MCP surface.
export type ViewStructureQuery = WithLocalOverlay<
  z.infer<typeof UpstreamViewStructureQuerySchema>
>;

export const BulkViewStructureSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.LOCAL_VIEW_STRUCTURE,
  ViewStructureQuerySchema,
  { maxQueries: 5 }
);
