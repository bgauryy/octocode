/**
 * GitLab Merge Requests
 *
 * Search for merge requests in GitLab.
 * Note: GitLab MRs use `iid` (project-scoped) vs `id` (global).
 *
 * @module gitlab/mergeRequests
 */

import type {
  GitLabAPIResponse,
  GitLabMergeRequestQuery,
  GitLabMergeRequest,
  GitLabMRNote,
} from './types.js';
import { getGitlab } from './client.js';
import { handleGitLabAPIError, createGitLabError } from './errors.js';
import { generateCacheKey, withDataCache } from '../utils/http/cache.js';
import {
  hasGitLabAllDiffs,
  isGitLabMRNote,
  isGitLabMergeRequest,
  parseGitLabArray,
} from './responseGuards.js';

/**
 * Merge request search result.
 */
export interface GitLabMRSearchResult {
  mergeRequests: GitLabMergeRequest[];
  pagination: {
    currentPage: number;
    totalPages?: number;
    perPage: number;
    totalMatches?: number;
    hasMore: boolean;
  };
}

/**
 * Search for GitLab merge requests.
 *
 * @param params - Search parameters
 * @param sessionId - Optional session ID for caching
 * @returns MR search results
 */
export async function searchGitLabMergeRequestsAPI(
  params: GitLabMergeRequestQuery,
  sessionId?: string
): Promise<GitLabAPIResponse<GitLabMRSearchResult>> {
  // Generate cache key
  const cacheKey = generateCacheKey(
    'gl-api-mrs',
    {
      projectId: params.projectId,
      iid: params.iid,
      state: params.state,
      authorUsername: params.authorUsername,
      assigneeUsername: params.assigneeUsername,
      labels: params.labels,
      sourceBranch: params.sourceBranch,
      targetBranch: params.targetBranch,
      createdAfter: params.createdAfter,
      updatedAfter: params.updatedAfter,
      orderBy: params.orderBy,
      sort: params.sort,
      perPage: params.perPage,
      page: params.page,
    },
    sessionId
  );

  return withDataCache<GitLabAPIResponse<GitLabMRSearchResult>>(
    cacheKey,
    async () => searchGitLabMergeRequestsAPIInternal(params),
    {
      shouldCache: value => 'data' in value && !('error' in value),
    }
  );
}

async function searchGitLabMergeRequestsAPIInternal(
  params: GitLabMergeRequestQuery
): Promise<GitLabAPIResponse<GitLabMRSearchResult>> {
  try {
    const gitlab = await getGitlab();

    const perPage = Math.min(params.perPage || 20, 100);
    const page = params.page || 1;

    // If fetching a specific MR by iid
    if (params.projectId && params.iid) {
      const mr = await gitlab.MergeRequests.show(params.projectId, params.iid);
      if (!isGitLabMergeRequest(mr)) {
        return createGitLabError(
          'Unexpected GitLab merge request response shape',
          502
        );
      }

      return {
        data: {
          mergeRequests: [mr],
          pagination: {
            currentPage: 1,
            totalPages: 1,
            perPage: 1,
            totalMatches: 1,
            hasMore: false,
          },
        },
        status: 200,
      };
    }

    const queryOptions: Record<string, unknown> = {
      state: params.state === 'all' ? undefined : params.state,
      authorUsername: params.authorUsername,
      assigneeUsername: params.assigneeUsername,
      labels: params.labels?.join(','),
      sourceBranch: params.sourceBranch,
      targetBranch: params.targetBranch,
      createdAfter: params.createdAfter,
      updatedAfter: params.updatedAfter,
      orderBy: params.orderBy || 'created_at',
      sort: params.sort || 'desc',
      perPage,
      page,
    };

    Object.keys(queryOptions).forEach(key => {
      if (queryOptions[key] === undefined) {
        delete queryOptions[key];
      }
    });

    let mergeRequests: GitLabMergeRequest[];

    if (params.projectId) {
      // Project-scoped search
      const rawMergeRequests = await gitlab.MergeRequests.all({
        projectId: params.projectId,
        ...queryOptions,
      });
      const parsedMergeRequests = parseGitLabArray(
        rawMergeRequests,
        isGitLabMergeRequest
      );
      if (!parsedMergeRequests) {
        return createGitLabError(
          'Unexpected GitLab merge request list response shape',
          502
        );
      }
      mergeRequests = parsedMergeRequests;
    } else {
      // Global search
      const rawMergeRequests = await gitlab.MergeRequests.all(queryOptions);
      const parsedMergeRequests = parseGitLabArray(
        rawMergeRequests,
        isGitLabMergeRequest
      );
      if (!parsedMergeRequests) {
        return createGitLabError(
          'Unexpected GitLab merge request list response shape',
          502
        );
      }
      mergeRequests = parsedMergeRequests;
    }

    const hasMore = mergeRequests.length === perPage;

    return {
      data: {
        mergeRequests,
        pagination: {
          currentPage: page,
          totalPages: hasMore ? page + 1 : page,
          perPage,
          totalMatches: mergeRequests.length,
          hasMore,
        },
      },
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Get comments/notes for a merge request.
 *
 * @param projectId - Project ID
 * @param mrIid - MR iid (project-scoped number)
 * @returns MR notes
 */
export async function getGitLabMRNotes(
  projectId: number | string,
  mrIid: number
): Promise<GitLabAPIResponse<GitLabMRNote[]>> {
  try {
    const gitlab = await getGitlab();

    const notes = parseGitLabArray(
      await gitlab.MergeRequestNotes.all(projectId, mrIid, {
        perPage: 100,
      }),
      isGitLabMRNote
    );
    if (!notes) {
      return createGitLabError(
        'Unexpected GitLab merge request notes response shape',
        502
      );
    }

    // Filter out system notes
    const userNotes = notes.filter(note => !note.system);

    return {
      data: userNotes,
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Get changes/diff for a merge request.
 *
 * @param projectId - Project ID
 * @param mrIid - MR iid
 * @returns MR changes
 */
export async function getGitLabMRChanges(
  projectId: number | string,
  mrIid: number
): Promise<GitLabAPIResponse<{ changes: unknown[] }>> {
  try {
    const gitlab = await getGitlab();
    if (!hasGitLabAllDiffs(gitlab.MergeRequests)) {
      return createGitLabError(
        'GitLab merge request diff API is unavailable',
        500
      );
    }

    const changes = await gitlab.MergeRequests.allDiffs(projectId, mrIid, {
      perPage: 100,
    });

    return {
      data: { changes: Array.isArray(changes) ? changes : [] },
      status: 200,
    };
  } catch (error) {
    return handleGitLabAPIError(error);
  }
}

/**
 * Transform GitLab MR to unified format.
 */
export function transformGitLabMergeRequest(mr: GitLabMergeRequest): {
  number: number;
  title: string;
  body: string | null;
  url: string;
  state: 'open' | 'closed' | 'merged';
  draft: boolean;
  author: string;
  assignees: string[];
  labels: string[];
  sourceBranch: string;
  targetBranch: string;
  createdAt: string;
  updatedAt: string;
  closedAt?: string;
  mergedAt?: string;
} {
  // Map GitLab state to unified state
  let state: 'open' | 'closed' | 'merged';
  if (mr.state === 'merged') {
    state = 'merged';
  } else if (mr.state === 'closed') {
    state = 'closed';
  } else {
    state = 'open';
  }

  return {
    number: mr.iid, // Use iid (project-scoped) like GitHub PR number
    title: mr.title,
    body: mr.description,
    url: mr.web_url,
    state,
    draft: mr.draft || mr.work_in_progress,
    author: mr.author.username,
    assignees: mr.assignees?.map(a => a.username) || [],
    labels: mr.labels || [],
    sourceBranch: mr.source_branch,
    targetBranch: mr.target_branch,
    createdAt: mr.created_at,
    updatedAt: mr.updated_at,
    closedAt: mr.closed_at || undefined,
    mergedAt: mr.merged_at || undefined,
  };
}
