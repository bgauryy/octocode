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
  PackageSearchQuerySchema,
  createBulkQuerySchema,
} from '@octocodeai/octocode-core';
import { STATIC_TOOL_NAMES } from '../tools/toolNames.js';

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
  });

export type GitHubPullRequestSearchQueryLocal = z.infer<
  typeof GitHubPullRequestSearchQueryLocalSchema
>;

export const GitHubPullRequestSearchBulkQueryLocalSchema =
  createBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
    GitHubPullRequestSearchQueryLocalSchema
  );

// ---------------------------------------------------------------------------
// packageSearch — default ecosystem to "npm" when the field is absent
// ---------------------------------------------------------------------------

const packageQueryWithEcosystemDefault = z.preprocess(val => {
  if (
    val &&
    typeof val === 'object' &&
    !Object.prototype.hasOwnProperty.call(val, 'ecosystem')
  ) {
    return { ...(val as Record<string, unknown>), ecosystem: 'npm' };
  }
  return val;
}, PackageSearchQuerySchema);

export const PackageSearchBulkQueryLocalSchema = createBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  packageQueryWithEcosystemDefault,
  { maxQueries: 5 }
);
