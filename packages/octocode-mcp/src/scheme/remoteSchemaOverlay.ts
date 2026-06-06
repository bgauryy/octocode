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
  createVerbosityFields,
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
    ...createVerbosityFields(),
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
      'Search terms AND-combined by GitHub. Each array element is a separate required term — do NOT put multi-word phrases in one element (split them: ["foo","bar"] not ["foo bar"]). Use a small set of distinctive identifiers.'
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
    page: relaxedPageNumberField
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

export const GitHubPullRequestSearchQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQuerySchema.omit({ limit: true }).extend({
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
      type: z
        .enum(['metadata', 'partialContent', 'fullContent'])
        .optional()
        .describe(
          'Gradual data mode — always start with "metadata" (default: titles, authors, file counts, no diffs) for triage. Step up to "partialContent" (targeted patches for specific files via partialContentMetadata) once candidate PRs are identified. Use "fullContent" only with a specific prNumber to avoid large payloads.'
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
          'Include PRs from archived repositories. Default (omitted/false) excludes them.'
        ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${DEFAULT_PAGE_SIZE} pull requests.`
        ),
      partialContentMetadata: z
        .array(
          z.object({
            file: z
              .string()
              .describe(
                'File path relative to repo root, exactly as returned in the metadata-mode file list (e.g. "src/utils/foo.ts").'
              ),
            additions: z
              .array(clampedInt(1, 1_000_000_000))
              .optional()
              .describe(
                'New-file line numbers to keep from the patch (e.g. [12,13,14]). Omit to include all additions.'
              ),
            deletions: z
              .array(clampedInt(1, 1_000_000_000))
              .optional()
              .describe(
                'Original-file line numbers to keep from the patch. Omit to include all deletions.'
              ),
          })
        )
        .optional()
        .describe(
          'Gradual per-file patch access for type="partialContent". Array of files to fetch patches for. Workflow: (1) call with type="metadata" to get the file list; (2) call again with type="partialContent" and list the target files here — batch multiple files in one array to minimize round-trips. Optionally scope to specific line numbers via additions/deletions.'
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
      'Package name to resolve through the npm registry before using GitHub tools.'
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
