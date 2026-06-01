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
  relaxedPaginationLimitField,
  relaxedPageNumberField,
  lineNumberField,
  charOffsetField,
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
    type: describeField(
      UpstreamFileContentQuerySchema.shape.type,
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
  /** Output format marker — only present when format='tsv' was requested. */
  format: z.literal('tsv').optional(),
  /** TSV column header list (only when format='tsv'). */
  columns: z.array(z.string()).optional(),
  /** TSV row payload as a single tab-delimited string (only when format='tsv'). */
  rows: z.string().optional(),
  /** Common directory the `path` cells are relative to in lean TSV output. */
  base: z.string().optional(),
  /** Columns hoisted out because every TSV row shared one value. */
  shared: z.record(z.string(), z.string()).optional(),
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
  UpstreamGitHubCodeSearchQuerySchema.extend({
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
    // GitHub code search hard ceilings: per_page ≤ 100, total ≤ 1000 results,
    // ≤ 10 pages. The field is bounded to the real per-page max (100) and
    // defaults to it; lower it deliberately when you only need a few hits.
    limit: clampedInt(1, 100)
      .describe(
        'Code-search results requested from GitHub per page. GitHub caps this at 100 per page (per_page); ' +
          'walk further pages with `page` up to the 1000-result / 10-page ceiling — beyond that, narrow the query. ' +
          'Default 100. Under verbosity="concise" the count is capped to 3. ' +
          'Independent of pagination: charOffset/charLength/responseCharLength bound serialized size separately ' +
          'and never truncate — they page. When both apply, the tighter wins.'
      )
      .default(100),
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
  /** Common directory the `path` cells are relative to in lean TSV output. */
  base: z.string().optional(),
  /** Columns hoisted out because every TSV row shared one value. */
  shared: z.record(z.string(), z.string()).optional(),
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
  UpstreamGitHubViewRepoStructureQuerySchema.extend({
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
    entriesPerPage: relaxedPaginationLimitField.default(100),
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
export const GitHubReposSearchSingleQueryLocalSchema =
  UpstreamGitHubReposSearchSingleQuerySchema.extend({
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
export const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema.extend({
    query: describeField(
      GitHubPullRequestSearchQuerySchema.shape.query,
      'Free-text PR search query. For PR archaeology, start with title keywords and matchScope=["title"].'
    ),
    prNumber: describeField(
      GitHubPullRequestSearchQuerySchema.shape.prNumber,
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
const PACKAGE_SEARCH_DEFAULT_LIMIT = 5;

const npmPackageQueryWithLimit = NpmPackageQuerySchema.extend({
  name: describeField(
    NpmPackageQuerySchema.shape.name,
    'Package name to resolve through the registry before using GitHub tools.'
  ),
  ecosystem: describeField(
    NpmPackageQuerySchema.shape.ecosystem,
    'Package registry ecosystem. Omitted defaults to "npm"; explicit non-npm values are rejected by this server.'
  ),
  // ONE result-count knob: `searchLimit` — OPTIONAL in the published schema
  // (not required) so a name-only call is valid; the default (5) is applied in
  // the preprocess below, not via `.default()` (a `.default()` on the clampedInt
  // preprocess leaks into JSON-schema `required[]`). The legacy `limit` alias is
  // no longer advertised — exposing both made two fields for one concept; a
  // caller that still passes `limit` is tolerated (preprocess maps it over).
  searchLimit: relaxedPaginationLimitField.describe(
    'Maximum package results to return per page. Default 5.'
  ),
  // Result-count cursor for keyword search: increment `page` to fetch matches
  // beyond the first searchLimit (maps to the registry `from` offset). Default 1.
  page: relaxedPageNumberField.describe(
    'Result page (1-based) for keyword search. Increment to fetch matches beyond the first searchLimit. Default 1.'
  ),
  verbosity: createVerbosityField(),
}).superRefine((data, ctx) => {
  // Reject non-npm ecosystems at schema layer (was a runtime check in
  // execution.ts). The discriminated upstream `NpmPackageQuerySchema` does
  // not enforce the literal 'npm' since callers can omit ecosystem entirely
  // (the preprocess fills it in below) — so the explicit guard lives here.
  const ecosystem = (data as { ecosystem?: string }).ecosystem;
  if (ecosystem !== undefined && ecosystem !== 'npm') {
    ctx.addIssue({
      code: z.ZodIssueCode.custom,
      message: `Unsupported ecosystem '${ecosystem}'. Only 'npm' is supported.`,
      path: ['ecosystem'],
    });
  }
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
    // Tolerate the legacy `limit` alias without advertising it: map it onto
    // the canonical `searchLimit` when the caller didn't supply searchLimit,
    // then drop it so the (stripping) object schema doesn't see a stray key.
    if (
      !Object.prototype.hasOwnProperty.call(next, 'searchLimit') &&
      typeof next.limit === 'number'
    ) {
      next.searchLimit = next.limit;
    }
    delete next.limit;
    // Apply the default here (not via `.default()` on the field) so the field
    // stays OPTIONAL in the published JSON schema instead of leaking into
    // `required[]`.
    if (typeof next.searchLimit !== 'number') {
      next.searchLimit = PACKAGE_SEARCH_DEFAULT_LIMIT;
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
} from '@octocodeai/octocode-core/schemas/outputs';

import { EvidenceSchema, tsvEnvelopeFields } from './tsvEnvelope.js';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';

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
