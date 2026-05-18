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
 * 2. packageSearch — defaults `ecosystem` to `"npm"` when omitted.
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

export const FileContentQueryLocalSchema =
  UpstreamFileContentQuerySchema.extend({
    charLength: localCharLengthField,
    matchStringContextLines: matchStringContextLinesField,
  });

export type FileContentQueryLocal = z.infer<typeof FileContentQueryLocalSchema>;

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_FETCH_CONTENT,
  FileContentQueryLocalSchema
);

// ---------------------------------------------------------------------------
// githubSearchCode
// ---------------------------------------------------------------------------

export const GitHubCodeSearchQueryLocalSchema =
  UpstreamGitHubCodeSearchQuerySchema.extend({
    charLength: localCharLengthField,
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
  });

export type GitHubCodeSearchQueryLocal = z.infer<
  typeof GitHubCodeSearchQueryLocalSchema
>;

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    GitHubCodeSearchQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubViewRepoStructure
// ---------------------------------------------------------------------------

export const GitHubViewRepoStructureQueryLocalSchema =
  UpstreamGitHubViewRepoStructureQuerySchema.extend({
    entriesPerPage: relaxedPaginationLimitField.default(20),
    entryPageNumber: relaxedPageNumberField.default(1),
  });

export type GitHubViewRepoStructureQueryLocal = z.infer<
  typeof GitHubViewRepoStructureQueryLocalSchema
>;

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    GitHubViewRepoStructureQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchRepositories
// ---------------------------------------------------------------------------

export const GitHubReposSearchSingleQueryLocalSchema =
  UpstreamGitHubReposSearchSingleQuerySchema.extend({
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
  });

export type GitHubReposSearchSingleQueryLocal = z.infer<
  typeof GitHubReposSearchSingleQueryLocalSchema
>;

export const GitHubReposSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_REPOSITORIES,
    GitHubReposSearchSingleQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// githubSearchPullRequests — "merged" is a valid state shorthand
// ---------------------------------------------------------------------------

export const GitHubPullRequestSearchQueryLocalSchema =
  GitHubPullRequestSearchQuerySchema.extend({
    state: z
      .enum(['open', 'closed', 'merged'])
      .optional()
      .describe(
        'Filter by PR state. ' +
          '"open" = open PRs. ' +
          '"closed" = closed PRs (includes merged). ' +
          '"merged" = merged PRs only (shorthand for closed + merged:true).'
      ),
    page: relaxedPageNumberField.default(1),
    limit: relaxedPaginationLimitField.default(10),
  });

export type GitHubPullRequestSearchQueryLocal = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;

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
  .describe('Maximum results to return. Maps to searchLimit internally.');

const packageQueryUnionWithLimit = z.discriminatedUnion('ecosystem', [
  NpmPackageQuerySchema.extend({ limit: packageLimitField }),
  PythonPackageQuerySchema.extend({ limit: packageLimitField }),
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
