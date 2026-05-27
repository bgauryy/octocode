/**
 * Remote Schema Overlay
 *
 * Local extensions to schemas shipped in `@octocodeai/octocode-core` for the
 * remote (GitHub, package registry) tools. Follows the same pattern as
 * `localSchemaOverlay.ts` and `lspSchemaOverlay.ts`.
 *
 * Changes applied here:
 *
 * 1. githubSearchPullRequests — adds `"merged"` to the `state` enum.
 *    The GitHub search API maps `state:"merged"` to `is:merged` server-side;
 *    the execution layer already casts state through with the wider union.
 *
 * 2. githubSearchPullRequests — improves `query`, `match`, and `sort` descriptions.
 *    Adds explicit PR archaeology strategy: use match=["title"] + sort="best-match"
 *    as the first step when searching for a PR by approximate title keyword.
 *
 * 3. githubSearchRepositories — adds `language` field.
 *    Maps to GitHub's language: qualifier (primary repo language auto-detected from
 *    file extensions). More reliable than topicsToSearch for language filtering;
 *    topics are self-reported and sparse.
 *
 * 4. githubSearchRepositories — fixes `updated` description.
 *    Corrects "metadata update" to "last code push" (pushed: qualifier, not updated:).
 *
 * 5. packageSearch — defaults `ecosystem` to `"npm"` when omitted.
 *    The upstream schema is a discriminated union that requires `ecosystem`.
 *    A `z.preprocess` step injects `ecosystem: "npm"` before the union runs,
 *    so callers that only supply `name` get npm behaviour without an error.
 */

import { z } from 'zod/v4';
import {
  GitHubPullRequestSearchQuerySchema,
  NpmPackageQuerySchema,
  PythonPackageQuerySchema,
  FileContentQuerySchema as UpstreamFileContentQuerySchema,
  GitHubCodeSearchQuerySchema as UpstreamGitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema as UpstreamGitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema as UpstreamGitHubReposSearchSingleQuerySchema,
  BulkCloneRepoSchema as UpstreamBulkCloneRepoSchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  describeShapeFields,
  localCharLengthField,
  matchStringContextLinesField,
  relaxedPaginationLimitField,
  relaxedPageNumberField,
} from './localSchemaOverlay.js';

// ---------------------------------------------------------------------------
// githubCloneRepo
// ---------------------------------------------------------------------------

/**
 * Relaxed version of BulkCloneRepoSchema.
 * Since UpstreamBulkCloneRepoSchema is already a bulk schema, we extract its element schema.
 */
export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  (UpstreamBulkCloneRepoSchema.shape.queries as z.ZodArray<z.ZodTypeAny>)
    .element
);

// ---------------------------------------------------------------------------
// githubGetFileContent
// ---------------------------------------------------------------------------

const FileContentQueryLocalSchema = UpstreamFileContentQuerySchema.extend({
  ...describeShapeFields(UpstreamFileContentQuerySchema.shape, {
    branch: 'Branch/tag/SHA (defaults to repo default branch)',
    path: 'File path from root, no leading slash, exact case',
    startLine: 'Start line (with endLine)',
    endLine: 'End line (with startLine)',
    fullContent: 'Return entire file',
    matchString: 'Search pattern to extract',
    charOffset: 'Pagination offset',
  }),
  charLength: localCharLengthField.describe('Max chars per page'),
  matchStringContextLines: matchStringContextLinesField.describe(
    'Context lines around match'
  ),
});

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryLocalSchema
);

/**
 * Strict mirror of the runtime `PaginationInfo` interface (src/types.ts).
 * Replaces the prior `Record<string, unknown>` placeholder so finalizers
 * pass their typed pagination through without an `as unknown as` cast.
 * Every field stays optional except the three the runtime always emits
 * (`currentPage`, `totalPages`, `hasMore`) so providers can supply any
 * combination of byte / char / file / entry / match counters.
 */
const PaginationInfoSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  hasMore: z.boolean(),
  byteOffset: z.number().optional(),
  byteLength: z.number().optional(),
  totalBytes: z.number().optional(),
  charOffset: z.number().optional(),
  charLength: z.number().optional(),
  totalChars: z.number().optional(),
  filesPerPage: z.number().optional(),
  totalFiles: z.number().optional(),
  entriesPerPage: z.number().optional(),
  totalEntries: z.number().optional(),
  matchesPerPage: z.number().optional(),
  totalMatches: z.number().optional(),
});

/**
 * Char-budget pagination descriptor emitted by the bulk finalizers
 * (`responsePagination` / per-query `outputPagination`).  Stricter than
 * `PaginationInfoSchema` because the finalizers always populate all six
 * fields — no optional gaps.
 */
const CharPaginationSchema = z.object({
  currentPage: z.number(),
  totalPages: z.number(),
  hasMore: z.boolean(),
  charOffset: z.number(),
  charLength: z.number(),
  totalChars: z.number(),
});

const PerQueryPaginationSchema = CharPaginationSchema.extend({
  id: z.string(),
});

/**
 * Structured non-fatal signal shared across the grouped GitHub tools.
 * Discriminated by `kind` so callers branch on enum identity rather than
 * inline magic strings — and so new kinds extend cleanly without breaking
 * existing consumers.  Two kinds today:
 *
 *  - `match-value-truncated` — `githubSearchCode` had to clip a single match
 *    value to honour `responseCharLength`.  Re-query with a larger budget or
 *    narrow the search to recover.
 *  - `content-truncated` — `githubGetFileContent` had to clip a file's
 *    content for the same reason. The file is still listed, but its `content`
 *    no longer includes everything past `truncatedAt`.
 */
const GroupedToolWarningSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('match-value-truncated'),
    groupId: z.string(),
    path: z.string(),
    fullValueLength: z.number(),
    truncatedAt: z.number(),
    recovery: z.string(),
  }),
  z.object({
    kind: z.literal('content-truncated'),
    groupId: z.string(),
    path: z.string(),
    fullContentLength: z.number(),
    truncatedAt: z.number(),
    recovery: z.string(),
  }),
]);

export type GroupedToolWarning = z.infer<typeof GroupedToolWarningSchema>;

const GitHubFetchFileEntrySchema = z.object({
  path: z.string(),
  content: z.string(),
  totalLines: z.number().optional(),
  resolvedBranch: z.string().optional(),
  pagination: PaginationInfoSchema.optional(),
  isPartial: z.boolean().optional(),
  startLine: z.number().optional(),
  endLine: z.number().optional(),
  lastModified: z.string().optional(),
  lastModifiedBy: z.string().optional(),
  warnings: z.array(z.string()).optional(),
});

const GitHubFetchDirectoryEntrySchema = z.object({
  path: z.string(),
  localPath: z.string(),
  fileCount: z.number(),
  totalSize: z.number(),
  files: z
    .array(
      z.object({
        path: z.string(),
        size: z.number(),
        type: z.string(),
      })
    )
    .optional(),
  cached: z.boolean().optional(),
  resolvedBranch: z.string().optional(),
});

export const GitHubFetchContentOutputLocalSchema = z.object({
  /** Output format marker — only present when format='tsv' was requested. */
  format: z.literal('tsv').optional(),
  /** TSV column header list (only when format='tsv'). */
  columns: z.array(z.string()).optional(),
  /** TSV row payload as a single tab-delimited string (only when format='tsv'). */
  rows: z.string().optional(),
  /** Cross-tool evidence metadata (kind / answerReady / confidence / complete). */
  evidence: EvidenceSchema,
  results: z.array(
    z.object({
      id: z.string(),
      owner: z.string(),
      repo: z.string(),
      files: z.array(GitHubFetchFileEntrySchema).optional(),
      directories: z.array(GitHubFetchDirectoryEntrySchema).optional(),
    })
  ),
  responsePagination: CharPaginationSchema.optional(),
  hints: z.array(z.string()).optional(),
  warnings: z.array(GroupedToolWarningSchema).optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        owner: z.string().optional(),
        repo: z.string().optional(),
        path: z.string().optional(),
        error: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
});

export type GitHubFetchContentOutputLocal = z.infer<
  typeof GitHubFetchContentOutputLocalSchema
>;

// ---------------------------------------------------------------------------
// githubSearchCode
// ---------------------------------------------------------------------------

const GitHubCodeSearchQueryLocalSchema =
  UpstreamGitHubCodeSearchQuerySchema.extend({
    ...describeShapeFields(UpstreamGitHubCodeSearchQuerySchema.shape, {
      keywordsToSearch:
        'Search terms (AND logic). match=file returns text_matches[]',
      path: 'Directory path (strict prefix)',
    }),
    charLength: localCharLengthField,
    page: relaxedPageNumberField.default(1).describe('Page number'),
    limit: relaxedPaginationLimitField.default(10).describe('Max results'),
  });

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    GitHubCodeSearchQueryLocalSchema
  );

// CharPaginationSchema and PerQueryPaginationSchema are declared near the top
// of this file so the fetch-content schema can reuse them; their definitions
// are not repeated here.

// Code-search warnings re-use the shared GroupedToolWarningSchema declared
// above (next to the fetch-content schema) so both tools speak the same
// vocabulary and new kinds extend cleanly.

/**
 * Flat output shape for githubSearchCode: results grouped by owner/repo,
 * matchIndices removed, single-page upstream pagination omitted by the executor.
 *
 * Char-level pagination metadata fields:
 *   - `outputPagination`: per-query metadata array, one entry per query that
 *     supplied `charLength`/`charOffset`.
 *   - `responsePagination`: top-level bulk slicing metadata, driven by
 *     `responseCharLength` / `responseCharOffset`.
 */
export const GitHubCodeSearchOutputLocalSchema = z.object({
  /** Output format marker — only present when format='tsv' was requested. */
  format: z.literal('tsv').optional(),
  /** TSV column header list (only when format='tsv'). */
  columns: z.array(z.string()).optional(),
  /** TSV row payload as a single tab-delimited string (only when format='tsv'). */
  rows: z.string().optional(),
  /** Cross-tool evidence metadata (kind / answerReady / confidence / complete). */
  evidence: EvidenceSchema,
  results: z.array(
    z.object({
      id: z.string(),
      owner: z.string(),
      repo: z.string(),
      matches: z.array(
        z.object({
          path: z.string(),
          value: z.string().optional(),
        })
      ),
    })
  ),
  pagination: z
    .object({
      currentPage: z.number(),
      totalPages: z.number(),
      perPage: z.number(),
      totalMatches: z.number(),
      hasMore: z.boolean(),
    })
    .optional(),
  outputPagination: z.array(PerQueryPaginationSchema).optional(),
  responsePagination: CharPaginationSchema.optional(),
  hints: z.array(z.string()).optional(),
  warnings: z.array(GroupedToolWarningSchema).optional(),
  /**
   * Per-query no-match signal. A query that ran successfully but produced
   * zero matches is reported here so the caller can disambiguate
   * "merged into an existing owner/repo group" from "actually empty" —
   * which would otherwise be invisible in `results[]`.
   */
  emptyQueries: z
    .array(
      z.object({
        id: z.string(),
        // Per-query empty-result recovery hints. Each entry names the
        // filters in play and suggests a concrete next move (drop a
        // filter, switch match mode, broaden keywords).
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        error: z.string(),
      })
    )
    .optional(),
});

export type GitHubCodeSearchOutputLocal = z.infer<
  typeof GitHubCodeSearchOutputLocalSchema
>;
/** @deprecated alias kept for source compat — prefer `GroupedToolWarning`. */
export type GitHubCodeSearchWarning = GroupedToolWarning;

// ---------------------------------------------------------------------------
// githubViewRepoStructure
// ---------------------------------------------------------------------------

const GitHubViewRepoStructureQueryLocalSchema =
  UpstreamGitHubViewRepoStructureQuerySchema.extend({
    ...describeShapeFields(UpstreamGitHubViewRepoStructureQuerySchema.shape, {
      branch: 'Branch/tag/SHA (defaults to repo default branch)',
      depth: '1 (current) | 2 (subdirs)',
    }),
    entriesPerPage: relaxedPaginationLimitField
      .default(20)
      .describe('Entries per page'),
    entryPageNumber: relaxedPageNumberField.default(1).describe('1-based page'),
  });

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    GitHubViewRepoStructureQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchRepositories
// ---------------------------------------------------------------------------

const GitHubReposSearchSingleQueryLocalSchema =
  UpstreamGitHubReposSearchSingleQuerySchema.extend({
    ...describeShapeFields(UpstreamGitHubReposSearchSingleQuerySchema.shape, {
      keywordsToSearch: 'Keywords (AND) across name/description/README',
      topicsToSearch: 'GitHub topic tags (self-reported, often sparse)',
      owner: 'Owner/org scope',
      stars: 'Stars: ">500", "100..500"',
      updated: 'Last code push (pushed: qualifier, not metadata-only)',
      match: '["name"|"description"|"readme"] — restrictive',
    }),
    language: z
      .string()
      .optional()
      .describe(
        'Primary language ("TypeScript", "Python", "Go"). More reliable than topicsToSearch for language filtering'
      ),
    updated: z
      .string()
      .optional()
      .describe('Last code push (pushed: qualifier, not metadata-only)'),
    page: relaxedPageNumberField.default(1).describe('Page number'),
    limit: relaxedPaginationLimitField.default(10).describe('Max repos'),
  });

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    GitHubReposSearchSingleQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchPullRequests — "merged" is a valid state shorthand
// ---------------------------------------------------------------------------

const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema.extend({
    ...describeShapeFields(GitHubPullRequestSearchQuerySchema.shape, {
      query: 'Search across title/body/comments (max 256 chars)',
      created: 'Date: ">=YYYY-MM-DD" or "YYYY-MM-DD..YYYY-MM-DD"',
      updated: 'Same format as created',
      closed: 'Same format as created',
      'merged-at': 'Same format as created',
      comments: 'Count: ">5", "10..20"',
      match: '["title"|"body"|"comments"]; default all three',
      order: 'desc | asc',
      withComments: 'Include discussions (expensive)',
      type: 'metadata | fullContent | partialContent',
    }),
    state: z
      .enum(['open', 'closed', 'merged'])
      .optional()
      .describe(
        '"open" | "closed" | "merged" (shorthand for closed + merged:true)'
      ),
    match: z
      .array(z.enum(['title', 'body', 'comments']))
      .optional()
      .describe('["title"|"body"|"comments"]; default all three'),
    sort: z
      .enum(['created', 'updated', 'best-match'])
      .optional()
      .describe('created | updated | best-match'),
    page: relaxedPageNumberField.default(1).describe('Page number'),
    limit: relaxedPaginationLimitField.default(10).describe('Max PRs'),
  });

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// packageSearch — default ecosystem to "npm" when the field is absent
// ---------------------------------------------------------------------------

const packageLimitField = relaxedPaginationLimitField
  .default(5)
  .describe('Max results (Python is always 1)');

const packageQueryUnionWithLimit = z.discriminatedUnion('ecosystem', [
  NpmPackageQuerySchema.extend({
    ...describeShapeFields(NpmPackageQuerySchema.shape, {
      ecosystem: '"npm" | "python" (defaults to "npm")',
    }),
    limit: packageLimitField,
    searchLimit: packageLimitField,
  }),
  PythonPackageQuerySchema.extend({
    ...describeShapeFields(PythonPackageQuerySchema.shape, {
      ecosystem: '"npm" | "python" (defaults to "npm")',
      pythonFetchMetadata:
        'Fetch full Python metadata (version/author/license)',
    }),
    limit: packageLimitField,
    searchLimit: packageLimitField,
  }),
]);

const packageQueryWithEcosystemDefault = z.preprocess(
  val => {
    if (
      val &&
      typeof val === 'object' &&
      !Object.prototype.hasOwnProperty.call(val, 'ecosystem')
    ) {
      return { ...(val as Record<string, unknown>), ecosystem: 'npm' };
    }
    return val;
  },
  packageQueryUnionWithLimit.transform(val => {
    // Map 'limit' to 'searchLimit' which is what the execution layer/upstream might expect
    const { limit, ...rest } = val as { limit: number; [key: string]: unknown };
    return { ...rest, searchLimit: limit };
  })
);

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  packageQueryWithEcosystemDefault,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output schema extensions — add peer-level `hints` and TSV envelope fields
// (`format`, `columns`, `rows`) to each upstream output schema. Wraps the
// upstream object so the bulk runner can emit these top-level keys without
// failing strict Zod validation.
// ---------------------------------------------------------------------------
import {
  GitHubSearchRepositoriesOutputSchema as UpstreamReposOutput,
  GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput,
  GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput,
  PackageSearchOutputSchema as UpstreamPackageOutput,
} from '@octocodeai/octocode-core';

import { EvidenceSchema, tsvEnvelopeFields } from './tsvEnvelope.js';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core';

const peerEnvelopeFields = {
  hints: z.array(z.string()).optional(),
  format: z.literal('tsv').optional(),
  columns: z.array(z.string()).optional(),
  rows: z.string().optional(),
  /** Cross-tool evidence metadata (kind / answerReady / confidence / complete). */
  evidence: EvidenceSchema,
} as const;

export const GitHubSearchRepositoriesOutputLocalSchema =
  UpstreamReposOutput.extend(peerEnvelopeFields);

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(peerEnvelopeFields);

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(peerEnvelopeFields);

export const PackageSearchOutputLocalSchema =
  UpstreamPackageOutput.extend(peerEnvelopeFields);

export const GitHubCloneRepoOutputLocalSchema =
  UpstreamCloneRepoOutput.extend(tsvEnvelopeFields);
