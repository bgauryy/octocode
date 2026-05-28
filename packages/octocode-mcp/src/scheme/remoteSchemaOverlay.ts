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
  FileContentQuerySchema as UpstreamFileContentQuerySchema,
  GitHubCodeSearchQuerySchema as UpstreamGitHubCodeSearchQuerySchema,
  GitHubViewRepoStructureQuerySchema as UpstreamGitHubViewRepoStructureQuerySchema,
  GitHubReposSearchSingleQuerySchema as UpstreamGitHubReposSearchSingleQuerySchema,
  BulkCloneRepoSchema as UpstreamBulkCloneRepoSchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  createRelaxedBulkQuerySchema,
  createVerbosityField,
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
 * Since UpstreamBulkCloneRepoSchema is already a bulk schema, we extract its
 * element schema and extend it with the cross-tool verbosity field.
 */
const CloneRepoElementSchema = (
  UpstreamBulkCloneRepoSchema.shape.queries as z.ZodArray<z.ZodTypeAny>
).element as unknown as z.ZodObject<z.ZodRawShape>;

// Clone is a one-shot side-effecting action — no verbosity field.
const CloneRepoQueryLocalSchema = CloneRepoElementSchema;

export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  CloneRepoQueryLocalSchema
);

// ---------------------------------------------------------------------------
// githubGetFileContent
// ---------------------------------------------------------------------------

// Description text for every field lives upstream in
// octocode-core/src/resources/tools/githubGetFileContent.ts — no overlay
// redescribes here. Only pagination defaults / numeric ranges remain.
const FileContentQueryLocalSchema = UpstreamFileContentQuerySchema.extend({
  charLength: localCharLengthField,
  matchStringContextLines: matchStringContextLinesField,
  verbosity: createVerbosityField(),
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
 * existing consumers.  Three kinds today:
 *
 *  - `match-value-truncated` — `githubSearchCode` had to clip a single match
 *    value to honour `responseCharLength`.  Re-query with a larger budget or
 *    narrow the search to recover.
 *  - `content-truncated` — `githubGetFileContent` had to clip a file's
 *    content for the same reason. The file is still listed, but its `content`
 *    no longer includes everything past `truncatedAt`.
 *  - `verbosity-downgrade` — an explicit caller option was capped or coerced
 *    because the caller requested `verbosity:"ultra"` (e.g. `limit > 3`,
 *    `fullContent=true`, `npmFetchMetadata=true`). The response still
 *    succeeded; the warning names which field was overridden so the agent
 *    can re-call with `basic` if it needs the full payload.
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
  z.object({
    kind: z.literal('verbosity-downgrade'),
    field: z.string(),
    detail: z.string(),
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

// Field descriptions are upstream (githubSearchCode.ts). Overlay supplies
// only pagination defaults and the local char-budget field.
const GitHubCodeSearchQueryLocalSchema =
  UpstreamGitHubCodeSearchQuerySchema.extend({
    charLength: localCharLengthField,
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
    verbosity: createVerbosityField(),
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

// Field descriptions are upstream (githubViewRepoStructure.ts). Overlay
// supplies only pagination defaults.
const GitHubViewRepoStructureQueryLocalSchema =
  UpstreamGitHubViewRepoStructureQuerySchema.extend({
    entriesPerPage: relaxedPaginationLimitField.default(20),
    entryPageNumber: relaxedPageNumberField.default(1),
    verbosity: createVerbosityField(),
  });

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    GitHubViewRepoStructureQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchRepositories
// ---------------------------------------------------------------------------

// Field descriptions are upstream (githubSearchRepositories.ts). Overlay
// supplies pagination defaults only; `language` is kept relaxed as an
// optional string so the bulk relaxer accepts it.
const GitHubReposSearchSingleQueryLocalSchema =
  UpstreamGitHubReposSearchSingleQuerySchema.extend({
    language: z.string().optional(),
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
    verbosity: createVerbosityField(),
  });

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    GitHubReposSearchSingleQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchPullRequests — "merged" is a valid state shorthand
// ---------------------------------------------------------------------------

// Field descriptions are upstream (githubSearchPullRequests.ts). Overlay
// keeps only:
//  - the `state` enum tightening with "merged" shorthand
//  - the `matchScope` array enum (upstream rename from `match`)
//  - the `sort` enum tightening
//  - pagination defaults
const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema.extend({
    state: z.enum(['open', 'closed', 'merged']).optional(),
    matchScope: z.array(z.enum(['title', 'body', 'comments'])).optional(),
    sort: z.enum(['created', 'updated', 'best-match']).optional(),
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
    verbosity: createVerbosityField(),
  });

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// packageSearch — npm only; defaults ecosystem to "npm" when the field is absent
// ---------------------------------------------------------------------------

// Field descriptions are upstream (packageSearch.ts). Overlay supplies the
// pagination default and accepts a `limit` alias that the preprocess below
// re-maps to `searchLimit` for upstream compatibility.
const packageLimitField = relaxedPaginationLimitField.default(5);

const npmPackageQueryWithLimit = NpmPackageQuerySchema.extend({
  limit: packageLimitField,
  searchLimit: packageLimitField,
  verbosity: createVerbosityField(),
});

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
  npmPackageQueryWithLimit.transform(val => {
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
