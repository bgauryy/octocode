import { z } from 'zod';
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
  describeField,
  contextLinesField,
  relaxedPageNumberField,
  lineNumberField,
  depthField,
  DEFAULT_PAGE_SIZE,
  STRUCTURE_PAGE_SIZE,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from './localSchemaOverlay.js';
import { validateFileContentExtractionMode } from './fileContentModeValidation.js';

const CloneRepoElementSchema = (
  UpstreamBulkCloneRepoSchema.shape.queries as z.ZodArray<z.ZodTypeAny>
).element as unknown as z.ZodObject<z.ZodRawShape>;

export const CloneRepoQueryLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  CloneRepoElementSchema.extend({
    ...optionalMetaFields,
  })
);

export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  CloneRepoQueryLocalSchema
);

export const FileContentQueryBaseLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  UpstreamFileContentQuerySchema.extend({
    ...optionalMetaFields,
    type: z.enum(['file', 'directory']).optional(),
    startLine: lineNumberField,
    endLine: lineNumberField,
    matchStringContextLines: contextLinesField,
    charOffset: clampedInt(0, 100_000_000)
      .optional()
      .describe(
        'Character offset for file-content pagination. Use the returned pagination charOffset+charLength hint to continue, or jump near the tail for large files.'
      ),
    charLength: clampedInt(1, 50_000)
      .optional()
      .describe(
        'Character page size for file-content pagination. Lower it for compact previews; raise it up to 50k when you need a larger contiguous chunk.'
      ),
    signaturesOnly: z
      .boolean()
      .optional()
      .describe(
        'Extract only the structural skeleton of the file: imports, function/class/interface/type signatures — bodies are dropped. Saves 80–95% tokens. Use for structure exploration; follow up with startLine/endLine to read specific bodies.'
      ),
    minify: z
      .boolean()
      .optional()
      .describe(
        'Control minification of returned content. Default true — comments and redundant whitespace are stripped for token efficiency. Pass false to get the raw unprocessed content (useful for debugging or when exact formatting matters).'
      ),
  })
);

export const FileContentQueryLocalSchema =
  FileContentQueryBaseLocalSchema.superRefine(
    validateFileContentExtractionMode
  );

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryBaseLocalSchema
);

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
  nextCharOffset: z.number().optional(),
  filesPerPage: z.number().optional(),
  totalFiles: z.number().optional(),
  entriesPerPage: z.number().optional(),
  totalEntries: z.number().optional(),
  matchesPerPage: z.number().optional(),
  totalMatches: z.number().optional(),
});

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
  matchNotFound: z.boolean().optional(),
  searchedFor: z.string().optional(),
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
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  evidence: EvidenceSchema,
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      owner: z.string(),
      repo: z.string(),
      files: z.array(GitHubFetchFileEntrySchema).optional(),
      directories: z.array(GitHubFetchDirectoryEntrySchema).optional(),
    })
  ),
  hints: z.array(z.string()).optional(),
  warnings: z.array(z.looseObject({ kind: z.string() })).optional(),
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

export const GitHubCodeSearchQueryLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
  UpstreamGitHubCodeSearchQuerySchema.omit({ limit: true }).extend({
    ...optionalMetaFields,
    keywordsToSearch: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.keywordsToSearch,
      'Search terms AND-combined by GitHub. Each array element is a separate required term — do NOT put multi-word phrases in one element (split them: ["foo","bar"] not ["foo bar"]). Use a small set of distinctive identifiers; scope-only searches are usually low-signal.'
    ),
    owner: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.owner,
      'Optional GitHub owner/org scope. Pair with repo to search one repository.'
    ),
    repo: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.repo,
      'Optional repository scope. Use with owner to avoid broad global searches.'
    ),
    extension: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.extension,
      'Optional extension filter. Pass without a dot for clarity, e.g. "ts"; a leading dot is tolerated.'
    ),
    filename: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.filename,
      'Optional filename filter (GitHub `filename:` qualifier). Matches files whose name equals or contains this value — e.g. "Button.tsx" or "jest.config". Use this to target a specific file. Combine with `path` to restrict to a directory.'
    ),
    path: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.path,
      'Optional directory prefix filter (GitHub `path:` qualifier). Matches any file whose full repo path starts with this prefix — it is NOT a file path. Use `filename` to target one file; combine both to narrow to a directory. Passing a file-like path (e.g. "src/agent.ts") auto-extracts the filename part — but explicit `filename` is clearer and more reliable.'
    ),
    match: describeField(
      UpstreamGitHubCodeSearchQuerySchema.shape.match,
      'Where to search: "file" (default) searches file contents; "path" searches the file path/name only — useful for finding files by name pattern without any keyword. When omitted, GitHub searches content.'
    ),
    page: z
      .number()
      .int()
      .min(1)
      .max(1000)
      .optional()
      .default(1)
      .describe(
        `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} matches. Use page=2, page=3, … to walk through results.`
      ),
  })
);

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    GitHubCodeSearchQueryLocalSchema
  );

export const GitHubCodeSearchOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  evidence: EvidenceSchema,
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      queryId: z.string().optional(),
      owner: z.string(),
      repo: z.string(),
      matches: z.array(
        z.object({
          path: z.string().describe('Repo-relative file path of the match.'),
          value: z
            .string()
            .optional()
            .describe(
              'Code snippet returned by GitHub for this match. NOT the full file — use githubGetFileContent to read the full file.'
            ),
          pathOnly: z
            .boolean()
            .optional()
            .describe(
              'True when GitHub returned a path match but no text snippet. Use githubGetFileContent with matchString to inspect content.'
            ),
          matchIndices: z
            .array(
              z
                .object({ start: z.number(), end: z.number() })
                .describe(
                  'Character offsets within the `value` snippet string (not line numbers in the file).'
                )
            )
            .optional()
            .describe(
              'Character-offset spans inside the `value` snippet that highlight the matched terms. These are NOT line numbers — use githubGetFileContent with matchString to get exact line positions.'
            ),
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
  hints: z.array(z.string()).optional(),
  emptyQueries: z
    .array(
      z.object({
        id: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        error: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
});

export type GitHubCodeSearchOutputLocal = z.infer<
  typeof GitHubCodeSearchOutputLocalSchema
>;

export const GitHubViewRepoStructureQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    UpstreamGitHubViewRepoStructureQuerySchema.omit({
      entriesPerPage: true,
      entryPageNumber: true,
    }).extend({
      ...optionalMetaFields,
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
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} entries. Use page=2, page=3, … to walk through large directories.`
        ),
      itemsPerPage: clampedInt(1, 200)
        .optional()
        .describe('Entries per page for repository structure pagination.'),
      depth: depthField,
    })
  );

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    GitHubViewRepoStructureQueryLocalSchema
  );

export const GitHubReposSearchSingleQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    UpstreamGitHubReposSearchSingleQuerySchema.omit({ limit: true }).extend({
      ...optionalMetaFields,
      keywordsToSearch: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.keywordsToSearch,
        'Repository name/description keywords — each array element is a separate AND term. Do NOT use multi-word phrases in one element (["react","hooks"] not ["react hooks"]). Prefer fewer, distinctive terms.'
      ),
      topicsToSearch: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.topicsToSearch,
        'Self-reported GitHub topics. Useful but sparse; language is more reliable for language filtering.'
      ),
      owner: describeField(
        UpstreamGitHubReposSearchSingleQuerySchema.shape.owner,
        'Optional owner/org scope for repository discovery. Supply owner without keywordsToSearch to enumerate ALL repositories in an org or user account (uses the listing endpoint, bypasses the 1 000-result search cap). Supply owner WITH keywords to scope a keyword search to that org.'
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
          'Include archived repositories. Default (omitted/false) excludes them. Set true to find archived/deprecated projects.'
        ),
      sort: z
        .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
        .optional()
        .describe(
          'Sort field for repository results. Omit (or "best-match") for relevance ranking.'
        ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} repositories.`
        ),
    })
  );

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    GitHubReposSearchSingleQueryLocalSchema
  );

const PrPartialContentMetadataLocalSchema = z.object({
  file: z
    .string()
    .describe(
      'File path relative to repo root, exactly as returned in metadata/changedFiles output (e.g. "src/utils/foo.ts").'
    ),
  additions: z
    .array(clampedInt(1, 1_000_000_000))
    .optional()
    .describe('New-file line numbers to keep from the patch.'),
  deletions: z
    .array(clampedInt(1, 1_000_000_000))
    .optional()
    .describe('Original-file line numbers to keep from the patch.'),
});

const PrContentSelectorLocalSchema = z
  .object({
    metadata: z.boolean().optional(),
    body: z.boolean().optional(),
    changedFiles: z.boolean().optional(),
    patches: z
      .object({
        mode: z.enum(['none', 'selected', 'all']).optional(),
        files: z.array(z.string()).optional(),
        ranges: z.array(PrPartialContentMetadataLocalSchema).optional(),
      })
      .optional(),
    comments: z
      .object({
        discussion: z.boolean().optional(),
        reviewInline: z.boolean().optional(),
        includeBots: z.boolean().optional(),
        file: z.string().optional(),
      })
      .optional(),
    reviews: z.boolean().optional(),
    commits: z
      .object({
        list: z.boolean().optional(),
        includeFiles: z.boolean().optional(),
      })
      .optional(),
  })
  .optional()
  .describe(
    'Explicit PR content selector. Use for smart, paginated PR review: request body, changedFiles, selected/all patches, comments, reviews, or commits by need. Broad searches still return metadata only; re-call with prNumber for content.'
  );

export const GitHubPullRequestSearchQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQuerySchema.omit({
      limit: true,
      type: true,
      withComments: true,
      withCommits: true,
      partialContentMetadata: true,
    }).extend({
      ...optionalMetaFields,
      query: describeField(
        GitHubPullRequestSearchQuerySchema.shape.query,
        'Free-text PR search query. For PR archaeology, start with title keywords and matchScope=["title"].'
      ),
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
      state: z
        .enum(['open', 'closed', 'merged'])
        .optional()
        .describe(
          'PR state filter. "merged" emits is:merged in GitHub search (merged PRs only). "closed" returns all closed PRs (merged + unmerged). "open" for active PRs. Omit to search across all states.'
        ),
      matchScope: z
        .array(z.enum(['title', 'body', 'comments']))
        .optional()
        .describe(
          'Text fields searched by query. Use ["title"] first for PR archaeology; comments are slower/noisier.'
        ),
      sort: z
        .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
        .optional(),
      archived: z
        .boolean()
        .optional()
        .describe(
          'Include PRs from archived repositories. Default (omitted/false) excludes them.'
        ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `PR search result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} pull requests.`
        ),
      filePage: relaxedPageNumberField
        .optional()
        .describe(
          'Changed-file / patch file page for direct prNumber content requests.'
        ),
      commentPage: relaxedPageNumberField
        .optional()
        .describe('Comment page for direct prNumber content.requests.'),
      commitPage: relaxedPageNumberField
        .optional()
        .describe('Commit page for direct prNumber commit requests.'),
      itemsPerPage: clampedInt(1, 100)
        .optional()
        .describe('Items per page for changed files, comments, or commits.'),
      reviewMode: z
        .enum(['summary', 'full'])
        .optional()
        .describe(
          'Convenience mode for PR review. "full" requests metadata, body, changed files, patches, comments, reviews, and commits as a paginated review packet.'
        ),
      content: PrContentSelectorLocalSchema,
      charOffset: clampedInt(0, 100_000_000)
        .optional()
        .describe(
          'Character offset for paginated PR bodies/comment bodies in search results. Use the returned nextCharOffset to continue without losing data.'
        ),
      charLength: clampedInt(1, 50_000)
        .optional()
        .describe(
          'Character page size for PR bodies/comment bodies in search results. Broad PR searches default to compact body/comment windows; direct prNumber lookups return full details.'
        ),
      label: z
        .union([z.string(), z.array(z.string())])
        .optional()
        .describe(
          'Filter PRs by GitHub label name(s). String for one label, array for multiple. Labels with spaces are quoted automatically — e.g. label: "Pages Router" or label: ["bug", "enhancement"]. Use the exact label name as it appears on GitHub.'
        ),
      labels: z
        .union([z.string(), z.array(z.string())])
        .optional()
        .describe(
          'Alias for `label` (singular). Prefer using `label` directly. Both are accepted to avoid silent parameter ignore.'
        ),
      includeBots: z
        .boolean()
        .optional()
        .describe(
          'Include bot-authored comments (e.g. CI bots, Vercel, CodeRabbit). Default false — bot comments are filtered to reduce noise.'
        ),
      minify: z
        .boolean()
        .optional()
        .describe(
          'Control minification of PR content (patches, body, comments). Default true — comments and redundant whitespace are stripped from code patches for token efficiency. Pass false to get raw unprocessed diffs.'
        ),
    })
  );

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

const npmPackageQueryWithLimit = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  NpmPackageQuerySchema.omit({
    ecosystem: true,
    searchLimit: true,
  }).extend({
    ...optionalMetaFields,
    name: describeField(
      NpmPackageQuerySchema.shape.name,
      'Exact npm package name or npm keyword query. Output is compact and includes GitHub owner/repo, sourceRoot, entrypoints, and researchTargets when available.'
    ),
    npmFetchMetadata: describeField(
      NpmPackageQuerySchema.shape.npmFetchMetadata,
      'Fetch heavier npm metadata when needed; response still summarizes descriptions and exposes research handoff fields instead of dumping dependency trees.'
    ),
    page: relaxedPageNumberField.describe(
      `Result page (1-based). Exact package-name lookups return one canonical package; keyword searches use page to walk registry results (up to ${DEFAULT_PAGE_SIZE} per page).`
    ),
  })
);

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
    return next;
  }
  return val;
}, PackageSearchQueryLocalSchema);

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  packageQueryWithEcosystemDefault,
  { maxQueries: 5 }
);

import {
  GitHubSearchRepositoriesOutputSchema as UpstreamReposOutput,
  GitHubSearchPullRequestsOutputSchema as UpstreamPRsOutput,
  GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput,
  PackageSearchOutputSchema as UpstreamPackageOutput,
} from '@octocodeai/octocode-core/schemas/outputs';

import { EvidenceSchema, responseEnvelopeFields } from './responseEnvelope.js';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';

const peerEnvelopeFields = responseEnvelopeFields;

const LocalRepositoryDetailSchema = z
  .object({
    owner: z.string(),
    repo: z.string(),
    fullName: z.string(),
    stars: z.number().optional(),
    forks: z.number().optional(),
    openIssues: z.number().optional(),
    language: z.string().optional(),
    description: z.string().optional(),
    pushedAt: z.string().optional(),
    createdAt: z.string().optional(),
    defaultBranch: z.string().optional(),
    topics: z.array(z.string()).optional(),
    visibility: z.string().optional(),
  })
  .passthrough();

export const GitHubSearchRepositoriesOutputLocalSchema =
  UpstreamReposOutput.extend({
    ...peerEnvelopeFields,
    data: z
      .object({
        repositories: z.array(z.string()),
        repositoryDetails: z.array(LocalRepositoryDetailSchema),
        pagination: z
          .object({
            currentPage: z.number(),
            totalPages: z.number(),
            hasMore: z.boolean(),
            perPage: z.number().optional(),
            totalMatches: z.number().optional(),
          })
          .optional(),
      })
      .optional(),
  });

export const GitHubSearchPullRequestsOutputLocalSchema =
  UpstreamPRsOutput.extend(peerEnvelopeFields);

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(peerEnvelopeFields);

export const PackageSearchOutputLocalSchema =
  UpstreamPackageOutput.extend(peerEnvelopeFields);

export const GitHubCloneRepoOutputLocalSchema = UpstreamCloneRepoOutput.extend(
  responseEnvelopeFields
);
