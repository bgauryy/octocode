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
} from '@octocodeai/octocode-core/schemas';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  createVerbosityField,
  describeField,
  localCharLengthField,
  contextLinesField,
  relaxedPageNumberField,
  lineNumberField,
  charOffsetField,
  itemsPerPageField,
  githubItemsPerPageField,
  githubApiLimitField,
  depthField,
} from './localSchemaOverlay.js';
import { validateFileContentExtractionMode } from './fileContentModeValidation.js';

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
export const CloneRepoQueryLocalSchema = CloneRepoElementSchema;

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
// The superRefine enforces the three-mode mutual exclusion at the schema
// layer (fullContent / matchString / startLine+endLine). Mirrors the local
// sibling at localSchemaOverlay.ts and replaces the silent coercion that
// used to live in providerMappers.ts (where conflicting inputs were
// dropped without warning).
// Base (relaxed) per-query shape — NO extraction-mode mutex. The bulk envelope
// wraps THIS so a malformed query doesn't reject the whole batch at MCP
// input-validation time; the executor validates each query against the strict
// schema below and emits a per-query error (bulk contract: siblings still run).
export const FileContentQueryBaseLocalSchema =
  UpstreamFileContentQuerySchema.extend({
    owner: describeField(
      UpstreamFileContentQuerySchema.shape.owner,
      'GitHub repository owner or organization.'
    ),
    repo: describeField(
      UpstreamFileContentQuerySchema.shape.repo,
      'GitHub repository name without the owner.'
    ),
    path: describeField(
      UpstreamFileContentQuerySchema.shape.path,
      'Repository-relative file path, or directory path when type="directory".'
    ),
    branch: describeField(
      UpstreamFileContentQuerySchema.shape.branch,
      'Branch, tag, or commit SHA. Omit to resolve the repository default branch.'
    ),
    // Tighten the upstream free-string `type` to its closed enum — a typo like
    // "directroy" now fails validation instead of opaquely at runtime.
    type: z
      .enum(['file', 'directory'])
      .optional()
      .describe(
        'Content mode: "file" for a file slice, "directory" to fetch a subtree to disk. Directory mode requires ENABLE_LOCAL=true and ENABLE_CLONE=true.'
      ),
    fullContent: describeField(
      UpstreamFileContentQuerySchema.shape.fullContent,
      'Read the whole file. Mutually exclusive with matchString and startLine/endLine.'
    ),
    matchString: describeField(
      UpstreamFileContentQuerySchema.shape.matchString,
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
    charLength: localCharLengthField,
    charOffset: charOffsetField,
    matchStringContextLines: contextLinesField,
    verbosity: createVerbosityField(),
  });

// Strict per-query schema (base + mutex). The executor `safeParse`s each query
// against this to flag a mutex violation per-query.
export const FileContentQueryLocalSchema =
  FileContentQueryBaseLocalSchema.superRefine(
    validateFileContentExtractionMode
  );

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryBaseLocalSchema
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
 * existing consumers.
 *
 * There are NO truncation kinds. Oversized match values and file content are
 * windowed by char pagination (advance `responseCharOffset` / `charOffset`),
 * never clipped-with-a-marker — so there is nothing to warn about. The one
 * remaining kind is:
 *
 *  - `verbosity-downgrade` — an explicit caller option was capped or coerced
 *    because the caller requested `verbosity:"concise"` (e.g. `limit > 3`,
 *    `fullContent=true`, `npmFetchMetadata=true`). The response still
 *    succeeded; the warning names which field was overridden so the agent
 *    can re-call with `basic` if it needs the full payload.
 */
const GroupedToolWarningSchema = z.discriminatedUnion('kind', [
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
  /** Common directory the `path` cells are relativized against (lean output). */
  base: z.string().optional(),
  /** Scalar fields hoisted out of every leaf because they shared one value. */
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
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
export const GitHubCodeSearchQueryLocalSchema =
  UpstreamGitHubCodeSearchQuerySchema.omit({ limit: true }).extend({
    keywordsToSearch: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.keywordsToSearch,
      'Search terms combined by GitHub code search. Use a small set of distinctive identifiers or phrases.'
    ),
    owner: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.owner,
      'Optional GitHub owner/org scope. Pair with repo to search one repository.'
    ),
    repo: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.repo,
      'Optional repository scope. Use with owner to avoid broad global searches.'
    ),
    match: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.match,
      'Search target: "file" searches contents, "path" searches path/name metadata.'
    ),
    charLength: localCharLengthField,
    // Per-query char cursor — pairs with charLength so a caller can advance
    // within one query's matches exactly as the `Use charOffset=… on query
    // id=…` continuation hint instructs (symmetry with githubGetFileContent /
    // localGetFileContent, which both expose charOffset+charLength).
    charOffset: charOffsetField,
    page: relaxedPageNumberField.default(1),
    // Display page size (whole matches) — default 20, drives GitHub per_page so
    // fetched == shown. Under verbosity="concise" it is capped to 3.
    itemsPerPage: githubItemsPerPageField,
    // Raw GitHub per_page override (renamed from `limit`). Optional; omit to
    // track itemsPerPage. GitHub caps per_page at 100; walk pages with `page`.
    githubAPILimit: githubApiLimitField,
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
 *   - `perQueryPagination`: per-query char-window array (one entry per query
 *     that supplied `charLength`/`charOffset`). Named distinctly from the
 *     single-object `outputPagination` used by other tools — agents should
 *     consume the `hints` continuation strings rather than this field.
 *   - `responsePagination`: top-level bulk slicing metadata, driven by
 *     `responseCharLength` / `responseCharOffset`.
 */
export const GitHubCodeSearchOutputLocalSchema = z.object({
  /** Common directory the `path` cells are relativized against (lean output). */
  base: z.string().optional(),
  /** Scalar fields hoisted out of every leaf because they shared one value. */
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
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
          matchIndices: z
            .array(z.object({ start: z.number(), end: z.number() }))
            .optional(),
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
  // Per-query char-window cursors. This is an array (one entry per query that
  // triggered per-query char pagination) and is intentionally distinct from the
  // single-object `outputPagination` contract used by other tools. Agents should
  // use the `hints` continuation strings rather than reading this field directly.
  perQueryPagination: z.array(PerQueryPaginationSchema).optional(),
  responsePagination: CharPaginationSchema.optional(),
  hints: z.array(z.string()).optional(),
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

// ---------------------------------------------------------------------------
// githubViewRepoStructure
// ---------------------------------------------------------------------------

// Field descriptions are upstream (githubViewRepoStructure.ts). Overlay
// supplies only pagination defaults.
export const GitHubViewRepoStructureQueryLocalSchema =
  UpstreamGitHubViewRepoStructureQuerySchema.omit({
    entriesPerPage: true,
    // Page number is the unified cross-tool `page` (was entryPageNumber).
    entryPageNumber: true,
  }).extend({
    owner: describeField(
      UpstreamGitHubViewRepoStructureQuerySchema.shape.owner,
      'GitHub repository owner or organization.'
    ),
    repo: describeField(
      UpstreamGitHubViewRepoStructureQuerySchema.shape.repo,
      'GitHub repository name without the owner.'
    ),
    path: describeField(
      UpstreamGitHubViewRepoStructureQuerySchema.shape.path,
      'Repository-relative directory path to browse. Use "" or "." for the root.'
    ),
    branch: describeField(
      UpstreamGitHubViewRepoStructureQuerySchema.shape.branch,
      'Branch, tag, or commit SHA. Omit to use the repository default branch.'
    ),
    // Entries are the atomic item → default 100 so typical repos return in one
    // page without a follow-up entryPageNumber=2 call.
    itemsPerPage: clampedInt(1, 200)
      .default(100)
      .describe('Entries returned per page (1–200). Default 100.'),
    page: relaxedPageNumberField.default(1),
    // Clamp the upstream unbounded `depth` (it otherwise serializes the
    // ±9e15 safe-integer sentinel — schema bloat + a validation gap). Matches
    // the local view-structure bound (0-20).
    depth: depthField,
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
export const GitHubReposSearchSingleQueryLocalSchema =
  UpstreamGitHubReposSearchSingleQuerySchema.omit({ limit: true }).extend({
    keywordsToSearch: describeField(
      UpstreamGitHubReposSearchSingleQuerySchema.shape.keywordsToSearch,
      'Repository name, description, or README keywords. Prefer language for language filtering.'
    ),
    topicsToSearch: describeField(
      UpstreamGitHubReposSearchSingleQuerySchema.shape.topicsToSearch,
      'Self-reported GitHub topics. Useful but sparse; language is more reliable for language filtering.'
    ),
    owner: describeField(
      UpstreamGitHubReposSearchSingleQuerySchema.shape.owner,
      'Optional owner/org scope for repository discovery.'
    ),
    language: z
      .string()
      .optional()
      .describe(
        'Primary repository language qualifier, based on GitHub language detection. Prefer this over topicsToSearch for language filters.'
      ),
    archived: z
      .boolean()
      .optional()
      .describe(
        'Include archived repositories. Default (omitted/false) excludes them — archived repos are otherwise invisible to repo search. Set true to find archived/deprecated projects (e.g. facebookexperimental/Recoil).'
      ),
    // Tighten the upstream free-string `sort` to its valid enum (matches the
    // PR-search `sort` pattern) — rejects typos instead of failing at the API.
    sort: z
      .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
      .optional()
      .describe(
        'Sort field for repository results. Omit (or "best-match") for relevance ranking.'
      ),
    page: relaxedPageNumberField.default(1),
    itemsPerPage: githubItemsPerPageField,
    githubAPILimit: githubApiLimitField,
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
export const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema.omit({ limit: true }).extend({
    query: describeField(
      GitHubPullRequestSearchQuerySchema.shape.query,
      'Free-text PR search query. For PR archaeology, start with title keywords and matchScope=["title"].'
    ),
    // Bound the upstream unbounded `prNumber` (avoids the ±9e15 sentinel).
    prNumber: clampedInt(1, 1_000_000_000)
      .optional()
      .describe(
        'Direct PR number lookup. Cheapest and most precise when known.'
      ),
    owner: describeField(
      GitHubPullRequestSearchQuerySchema.shape.owner,
      'GitHub repository owner or organization.'
    ),
    repo: describeField(
      GitHubPullRequestSearchQuerySchema.shape.repo,
      'GitHub repository name without the owner.'
    ),
    state: z.enum(['open', 'closed', 'merged']).optional(),
    // Tighten the upstream free-string `type` (the main cost lever) to its
    // closed enum so a typo fails validation, not silently at runtime.
    type: z
      .enum(['metadata', 'partialContent', 'fullContent'])
      .optional()
      .describe(
        'Cost lever: "metadata" (default, triage) → "partialContent" (named files/lines via partialContentMetadata) → "fullContent" (whole patch; tiny PRs or with prNumber).'
      ),
    matchScope: z
      .array(z.enum(['title', 'body', 'comments']))
      .optional()
      .describe(
        'Text fields searched by query. Use ["title"] first for PR archaeology; comments are slower/noisier.'
      ),
    sort: z.enum(['created', 'updated', 'best-match']).optional(),
    archived: z
      .boolean()
      .optional()
      .describe(
        'Include PRs from archived repositories. Default (omitted/false) excludes them. Set true for PR archaeology on archived/deprecated projects.'
      ),
    page: relaxedPageNumberField.default(1),
    itemsPerPage: githubItemsPerPageField,
    githubAPILimit: githubApiLimitField,
    // Bound the upstream unbounded char-pagination knobs (the PR tool supports
    // them but upstream left them as bare ints → ±9e15 sentinel).
    charLength: localCharLengthField,
    charOffset: charOffsetField,
    // Re-declare partialContentMetadata so its nested line-number arrays carry
    // bounds. Upstream leaves additions/deletions as bare `z.number().int()`,
    // which serialize the ±9e15 safe-integer sentinel into the published schema
    // and accept absurd/negative line numbers. Diff line numbers are positive
    // and share the [1, 1e9] ceiling used by startLine/endLine elsewhere.
    partialContentMetadata: z
      .array(
        z.object({
          file: z.string(),
          additions: z.array(clampedInt(1, 1_000_000_000)).optional(),
          deletions: z.array(clampedInt(1, 1_000_000_000)).optional(),
        })
      )
      .optional(),
    verbosity: createVerbosityField(),
  });

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// packageSearch — npm; defaults ecosystem to "npm" when the field is absent
// ---------------------------------------------------------------------------

// Field descriptions are upstream (packageSearch.ts). Overlay exposes the
// cross-tool `itemsPerPage` page-size knob (default 20) and the ecosystem default.
// Upstream `searchLimit` is dropped: it is the OLD result-count knob (now replaced
// by `itemsPerPage`, the only wired field — common.ts reads `query.itemsPerPage`)
// AND it is an unbounded `z.number().int()` that would serialize the ±9007e15
// safe-integer sentinel into the published JSON schema. Omitting it both removes
// the dead surface and closes that numeric-bounds hole — mirroring how every
// GitHub schema omits its legacy `limit`.
const npmPackageQueryWithLimit = NpmPackageQuerySchema.omit({
  ecosystem: true,
  searchLimit: true,
}).extend({
  name: describeField(
    NpmPackageQuerySchema.shape.name,
    'Package name to resolve through the registry before using GitHub tools.'
  ),
  ecosystem: z
    .literal('npm')
    .optional()
    .describe(
      'Package registry ecosystem. Omitted defaults to "npm"; only "npm" is supported.'
    ),
  // ONE result-count knob: `itemsPerPage` (the cross-tool page-size name).
  itemsPerPage: itemsPerPageField,
  // `page` is accepted for forward-compatibility but only page=1 is implemented
  // (no registry cursor is threaded through). To control result count, use
  // `itemsPerPage` instead.
  page: relaxedPageNumberField.describe(
    'Result page (1-based). Only page=1 is currently implemented; `itemsPerPage` is the correct lever to control result count.'
  ),
  verbosity: createVerbosityField(),
});

export const PackageSearchQueryLocalSchema = npmPackageQueryWithLimit;

const packageQueryWithEcosystemDefault = z.preprocess(val => {
  if (val && typeof val === 'object') {
    const record = val as Record<string, unknown>;
    const next = { ...record };
    if (
      !Object.prototype.hasOwnProperty.call(next, 'name') &&
      typeof next.packageName === 'string'
    ) {
      next.name = next.packageName;
    }
    if (!Object.prototype.hasOwnProperty.call(next, 'ecosystem')) {
      next.ecosystem = 'npm';
    }
    return next;
  }
  return val;
}, PackageSearchQueryLocalSchema);

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  packageQueryWithEcosystemDefault,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output schema extensions — add peer-level `hints`, `base`, and `evidence`
// to each upstream output schema. Wraps the upstream object so the bulk runner
// can emit these top-level keys without failing strict Zod validation.
// ---------------------------------------------------------------------------
import {
  GitHubSearchRepositoriesOutputSchema as UpstreamReposOutput,
  GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput,
  GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput,
  PackageSearchOutputSchema as UpstreamPackageOutput,
} from '@octocodeai/octocode-core/schemas/outputs';

import { EvidenceSchema, responseEnvelopeFields } from './responseEnvelope.js';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';

const peerEnvelopeFields = responseEnvelopeFields;

export const GitHubSearchRepositoriesOutputLocalSchema =
  UpstreamReposOutput.extend(peerEnvelopeFields);

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(peerEnvelopeFields);

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(peerEnvelopeFields);

export const PackageSearchOutputLocalSchema =
  UpstreamPackageOutput.extend(peerEnvelopeFields);

export const GitHubCloneRepoOutputLocalSchema = UpstreamCloneRepoOutput.extend(
  responseEnvelopeFields
);
