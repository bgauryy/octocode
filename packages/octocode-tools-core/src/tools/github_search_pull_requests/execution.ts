import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { handleCatchError, safeParseOrError } from '../utils.js';
import { createLazyProviderContext } from '../providerExecution.js';
import { handleReleasesMode } from './execution/releasesMode.js';
import { handleIssuesMode } from './execution/issuesMode.js';
import { handleCommitsMode } from './execution/commitsMode.js';
import { handlePullRequestsMode } from './execution/pullRequestsMode.js';
import { GitHubPullRequestSearchQueryLocalSchema } from './scheme.js';
import type { GitHubPullRequestSearchInput } from './execution/types.js';

// PR/issue SEARCH filters that only the prs/issues modes understand. With
// 179 input fields across four modes, a filter passed in the wrong mode must
// say so in-band instead of quietly returning unfiltered results.
const PR_SEARCH_ONLY_FIELDS = [
  'keywordsToSearch',
  'match',
  'state',
  'label',
  'milestone',
  'head',
  'base',
  'draft',
  'checks',
  'review',
  'reviewed-by',
  'review-requested',
  'commenter',
  'mentions',
  'involves',
  'team-mentions',
  'project',
  'reactions',
  'interactions',
  'comments',
  'locked',
  'visibility',
  'merged-at',
  'closed',
  'created',
  'updated',
] as const;

// Commit-history filters that only the commits mode consumes. `perPage` is
// deliberately excluded — it has a schema default (always "present") and is
// also honored by releases mode, so it is not commits-exclusive.
const COMMITS_ONLY_FIELDS = [
  'since',
  'until',
  'branch',
  'path',
  'includeDiff',
] as const;

// Foreign fields each mode silently ignores. prs/issues share one filter
// vocabulary; commits and releases each have their own. Releases understands
// neither PR-search nor commit-history filters.
const IGNORED_FIELDS_BY_MODE = {
  prs: COMMITS_ONLY_FIELDS,
  issues: COMMITS_ONLY_FIELDS,
  commits: PR_SEARCH_ONLY_FIELDS,
  releases: [...PR_SEARCH_ONLY_FIELDS, ...COMMITS_ONLY_FIELDS],
} as const satisfies Record<
  'prs' | 'issues' | 'commits' | 'releases',
  readonly string[]
>;

export function modeFieldWarnings(
  data: Record<string, unknown> | undefined,
  mode: 'prs' | 'issues' | 'commits' | 'releases'
): string[] {
  const present = IGNORED_FIELDS_BY_MODE[mode].filter(
    f => data?.[f] !== undefined
  );
  return present.length > 0
    ? [
        `Ignored in ${mode} mode (these filters have no effect here): ${present.join(', ')}.`,
      ]
    : [];
}

function withModeWarnings<T extends Record<string, unknown>>(
  res: T,
  warnings: string[]
): T {
  if (warnings.length === 0 || (res as { status?: string }).status === 'error')
    return res;
  const current = (res as { warnings?: unknown }).warnings;
  const existing = Array.isArray(current) ? (current as string[]) : [];
  return { ...res, warnings: [...existing, ...warnings] };
}

export async function searchMultipleGitHubPullRequests(
  args: ToolExecutionArgs<GitHubPullRequestSearchInput>
): Promise<CallToolResult> {
  const { queries, authInfo } = args;
  const getProviderContext = createLazyProviderContext(authInfo);

  return executeBulkOperation(
    queries,
    async (query: GitHubPullRequestSearchInput, _index: number) => {
      try {
        const parsed = safeParseOrError(
          GitHubPullRequestSearchQueryLocalSchema,
          query
        );
        if (parsed.ok === false) {
          return parsed.error;
        }

        const type = (parsed.data as { type?: string }).type;

        if (type === 'releases') {
          return withModeWarnings(
            await handleReleasesMode(query, parsed.data, authInfo),
            modeFieldWarnings(
              parsed.data as Record<string, unknown>,
              'releases'
            )
          );
        }

        if (type === 'issues') {
          return withModeWarnings(
            await handleIssuesMode(query, parsed.data, authInfo),
            modeFieldWarnings(parsed.data as Record<string, unknown>, 'issues')
          );
        }

        if (type === 'commits') {
          return withModeWarnings(
            await handleCommitsMode(query, parsed.data, authInfo),
            modeFieldWarnings(parsed.data as Record<string, unknown>, 'commits')
          );
        }

        return withModeWarnings(
          await handlePullRequestsMode(query, parsed.data, getProviderContext),
          modeFieldWarnings(parsed.data as Record<string, unknown>, 'prs')
        );
      } catch (error) {
        return handleCatchError(
          error,
          query,
          undefined,
          TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS
        );
      }
    },
    {
      toolName: TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS,
      keysPriority: [
        'pull_requests',
        'issues',
        'releases',
        'latest',
        'tagName',
        'publishedAt',
        'prerelease',
        'pagination',
        'total_count',
        'error',
      ],
    },
    args
  );
}
