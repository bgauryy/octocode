/**
 * GitLab Merge Request Search
 *
 * Extracted from GitLabProvider for better modularity.
 *
 * @module providers/gitlab/gitlabPullRequests
 */

import type {
  ProviderResponse,
  PullRequestQuery,
  PullRequestSearchResult,
  PullRequestItem,
} from '../types.js';

import {
  searchGitLabMergeRequestsAPI,
  getGitLabMRNotes,
  getGitLabMRChanges,
} from '../../gitlab/mergeRequests.js';
import {
  handleGitLabAPIResponse,
  mapGitLabMRState,
  parseGitLabProjectId,
} from './utils.js';
import {
  countPatchLineChanges,
  shapePullRequestFileChanges,
} from '../pullRequestFileChanges.js';
export { mapGitLabMRState as mapMRState, parseGitLabProjectId };

interface GitLabPaginationData {
  currentPage?: number;
  totalPages?: number;
  hasMore?: boolean;
  totalMatches?: number;
}

interface GitLabMRAssignee {
  username?: string;
}

interface GitLabMRNote {
  id: string | number;
  author?: { username?: string };
  body?: string;
  created_at?: string;
  updated_at?: string;
}

interface GitLabMRChange {
  old_path?: string;
  new_path?: string;
  diff?: string;
  new_file?: boolean;
  renamed_file?: boolean;
  deleted_file?: boolean;
}

interface GitLabMRData {
  iid: number;
  title: string;
  description?: string;
  web_url: string;
  state: string;
  draft?: boolean;
  work_in_progress?: boolean;
  author?: { username?: string };
  assignees?: GitLabMRAssignee[];
  labels?: string[];
  source_branch: string;
  target_branch: string;
  diff_refs?: { head_sha?: string; base_sha?: string };
  created_at?: string;
  updated_at?: string;
  closed_at?: string;
  merged_at?: string;
  user_notes_count?: number;
  _notes?: GitLabMRNote[];
  _changes?: GitLabMRChange[];
}

/**
 * Transform GitLab merge request result to unified format.
 */
export function transformPullRequestResult(
  mergeRequests: GitLabMRData[],
  pagination: GitLabPaginationData | undefined,
  query: PullRequestQuery
): PullRequestSearchResult {
  const items: PullRequestItem[] = mergeRequests.map(mr => {
    // Map GitLab state to unified state
    let state: 'open' | 'closed' | 'merged';
    if (mr.state === 'merged') {
      state = 'merged';
    } else if (mr.state === 'closed') {
      state = 'closed';
    } else {
      state = 'open';
    }

    const rawFileChanges =
      mr._changes?.map(change => {
        const path = change.new_path || change.old_path || '';
        const patch = change.diff || undefined;
        const patchCounts = countPatchLineChanges(patch);

        return {
          path,
          status: change.new_file
            ? 'added'
            : change.deleted_file
              ? 'removed'
              : change.renamed_file
                ? 'renamed'
                : 'modified',
          additions: patchCounts.additions,
          deletions: patchCounts.deletions,
          patch,
        };
      }) || [];

    const fileChangeSummary = shapePullRequestFileChanges(
      rawFileChanges,
      query
    );

    return {
      number: mr.iid,
      title: mr.title,
      body: mr.description ?? null,
      url: mr.web_url,
      state,
      draft: mr.draft || mr.work_in_progress || false,
      author: mr.author?.username || '',
      assignees: mr.assignees?.map(a => a.username ?? '') || [],
      labels: mr.labels || [],
      sourceBranch: mr.source_branch,
      targetBranch: mr.target_branch,
      sourceSha: mr.diff_refs?.head_sha,
      targetSha: mr.diff_refs?.base_sha,
      createdAt: mr.created_at ?? '',
      updatedAt: mr.updated_at ?? '',
      closedAt: mr.closed_at ?? undefined,
      mergedAt: mr.merged_at ?? undefined,
      commentsCount: mr.user_notes_count,
      comments: mr._notes?.map(note => ({
        id: String(note.id),
        author: note.author?.username || '',
        body: note.body ?? '',
        createdAt: note.created_at ?? '',
        updatedAt: note.updated_at ?? '',
      })),
      ...fileChangeSummary,
    };
  });

  return {
    items,
    totalCount: pagination?.totalMatches || items.length,
    pagination: {
      currentPage: pagination?.currentPage || 1,
      totalPages: pagination?.totalPages || 1,
      hasMore: pagination?.hasMore || false,
      totalMatches: pagination?.totalMatches,
    },
  };
}

/**
 * Search merge requests on GitLab.
 */
export async function searchPullRequests(
  query: PullRequestQuery,
  parseProjectId: (
    projectId?: string
  ) => number | string = parseGitLabProjectId,
  mapMRStateFn: (
    state?: string
  ) => 'opened' | 'closed' | 'merged' | 'all' | undefined = mapGitLabMRState
): Promise<ProviderResponse<PullRequestSearchResult>> {
  const projectId = query.projectId
    ? parseProjectId(query.projectId)
    : undefined;

  const gitlabQuery = {
    projectId,
    iid: query.number, // GitLab uses iid for project-scoped MR number
    state: mapMRStateFn(query.state),
    authorUsername: query.author,
    assigneeUsername: query.assignee,
    labels: query.labels,
    sourceBranch: query.headBranch,
    targetBranch: query.baseBranch,
    createdAfter: query.created,
    updatedAfter: query.updated,
    orderBy: (query.sort === 'created'
      ? 'created_at'
      : query.sort === 'updated'
        ? 'updated_at'
        : undefined) as 'created_at' | 'updated_at' | undefined,
    sort: query.order,
    perPage: query.limit,
    page: query.page,
  };

  const result = await searchGitLabMergeRequestsAPI(gitlabQuery);
  const providerResult = handleGitLabAPIResponse(
    result,
    'gitlab',
    data => data
  );

  if (!providerResult.data) {
    return {
      error: providerResult.error || 'No data returned from GitLab API',
      status: providerResult.status,
      provider: 'gitlab',
      hints: providerResult.hints,
      rateLimit: providerResult.rateLimit,
    };
  }

  let mergeRequests = providerResult.data.mergeRequests as GitLabMRData[];
  if (
    projectId &&
    mergeRequests.length > 0 &&
    (query.withComments || query.type)
  ) {
    mergeRequests = await Promise.all(
      mergeRequests.map(async mr => {
        let enrichedMergeRequest: GitLabMRData = mr as GitLabMRData;

        try {
          if (query.withComments) {
            const notesResult = await getGitLabMRNotes(projectId, mr.iid);
            const notesProviderResult = handleGitLabAPIResponse(
              notesResult,
              'gitlab',
              data => data
            );
            if (notesProviderResult.data) {
              enrichedMergeRequest = {
                ...enrichedMergeRequest,
                _notes: notesProviderResult.data,
              };
            }
          }

          if (query.type) {
            const changesResult = await getGitLabMRChanges(projectId, mr.iid);
            const changesProviderResult = handleGitLabAPIResponse(
              changesResult,
              'gitlab',
              data => data
            );
            if (changesProviderResult.data) {
              enrichedMergeRequest = {
                ...enrichedMergeRequest,
                _changes: (changesProviderResult.data.changes ||
                  []) as GitLabMRChange[],
              };
            }
          }
        } catch {
          // MR notes are optional enrichment; return the merge request without them.
        }
        return enrichedMergeRequest;
      })
    );
  }

  return {
    data: transformPullRequestResult(
      mergeRequests as GitLabMRData[],
      providerResult.data.pagination,
      query
    ),
    status: providerResult.status,
    provider: 'gitlab',
  };
}
