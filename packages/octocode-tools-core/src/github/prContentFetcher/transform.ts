import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PRCommentItem,
  DiffEntry,
  IssueSearchResultItem,
  PullRequestSimple,
  PullRequestItem,
} from '../githubAPI.js';
import { OctokitWithThrottling } from '../client.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import {
  createBasePRTransformation,
  normalizeOwnerRepo,
  applyPartialContentFilter,
} from '../prTransformation.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';
import {
  shouldFetchFileChanges,
  shouldFetchDiscussionComments,
  shouldFetchInlineComments,
  shouldFetchCommits,
  shouldFetchReviews,
  shouldEnrichPullRequestFromSearch,
  shouldIncludeBotComments,
} from './flags.js';
import {
  fetchPRComments,
  fetchPRInlineComments,
  fetchPRReviews,
} from './comments.js';
import { fetchPRFileChangesAPI, fetchPRCommitsWithFiles } from './commits.js';

// NOTE: transformPullRequestItemFromSearch and transformPullRequestItemFromREST
// are ~90% structurally duplicated (near-identical file-changes/comments/
// reviews/commits attach blocks). They were kept separate rather than merged
// into a shared `enrichPullRequestContent()` helper because the two differ in
// subtle ways that make a byte-for-byte-equivalent merge risky without
// changing behavior: the search variant gates its file-changes fetch inside
// the same try/catch as the `pulls.get` enrichment call (and derives
// additions/deletions/head/base/draft/merged_at from that enrichment
// response), while the REST variant fetches file changes unconditionally in
// its own try/catch and derives additions/deletions directly from the input
// item. A future pass could unify these, but it should be done with careful
// verification (or new tests pinning both code paths) rather than as part of
// this pure file-split refactor.

export async function transformPullRequestItemFromSearch(
  item: IssueSearchResultItem,
  params: GitHubPullRequestsSearchParams,
  octokit: InstanceType<typeof OctokitWithThrottling>
): Promise<GitHubPullRequestItem> {
  const rawItem = item as IssueSearchResultItem & { merged_at?: string | null };
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(rawItem);

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  let rawResponseChars = 0;

  if (item.pull_request && shouldEnrichPullRequestFromSearch(params)) {
    try {
      const { owner, repo } = normalizeOwnerRepo(params);

      if (owner && repo) {
        const prDetails = await octokit.rest.pulls.get({
          owner,
          repo,
          pull_number: item.number,
        });

        if (prDetails.data) {
          rawResponseChars += countSerializedChars(prDetails.data);
          result.head = prDetails.data.head?.ref;
          result.head_sha = prDetails.data.head?.sha;
          result.base = prDetails.data.base?.ref;
          result.base_sha = prDetails.data.base?.sha;
          result.draft = prDetails.data.draft ?? false;

          if (prDetails.data.merged_at) {
            result.merged_at = prDetails.data.merged_at;
          }

          result.additions = prDetails.data.additions ?? 0;
          result.deletions = prDetails.data.deletions ?? 0;

          if (!shouldFetchFileChanges(params)) {
            result.file_changes = {
              total_count: prDetails.data.changed_files ?? 0,
              files: [],
            };
          }

          if (shouldFetchFileChanges(params)) {
            const fileChanges = await fetchPRFileChangesAPI(
              owner,
              repo,
              item.number
            );

            if (fileChanges) {
              rawResponseChars += getRawResponseChars(fileChanges) ?? 0;
              fileChanges.files = applyPartialContentFilter(
                fileChanges.files,
                params
              ) as DiffEntry[];

              result.file_changes = fileChanges;
            }
          }
        }
      }
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (files): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  const wantDiscussion = shouldFetchDiscussionComments(params);
  const wantInline = shouldFetchInlineComments(params);
  if (wantDiscussion || wantInline) {
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
      const includeBots = shouldIncludeBotComments(params);
      const empty = (): Promise<{ comments: PRCommentItem[]; note?: string }> =>
        Promise.resolve({ comments: attachRawResponseChars([], 0) });
      const [
        { comments: discussionComments, note: discussionNote },
        { comments: inlineComments, note: inlineNote },
      ] = await Promise.all([
        wantDiscussion
          ? fetchPRComments(octokit, owner, repo, item.number, includeBots)
          : empty(),
        wantInline
          ? fetchPRInlineComments(
              octokit,
              owner,
              repo,
              item.number,
              includeBots
            )
          : empty(),
      ]);

      result.comments = [...discussionComments, ...inlineComments];
      rawResponseChars +=
        (getRawResponseChars(discussionComments) ?? 0) +
        (getRawResponseChars(inlineComments) ?? 0);

      const notes = [discussionNote, inlineNote].filter(
        (n): n is string => typeof n === 'string'
      );
      if (notes.length > 0) {
        result._sanitization_warnings = [
          ...(result._sanitization_warnings || []),
          ...notes,
        ];
      }
    }
  }

  if (shouldFetchReviews(params)) {
    try {
      const { owner, repo } = normalizeOwnerRepo(params);
      if (owner && repo) {
        const reviews = await fetchPRReviews(octokit, owner, repo, item.number);
        rawResponseChars += getRawResponseChars(reviews) ?? 0;
        result.reviews = reviews;
      }
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch reviews: ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  if (shouldFetchCommits(params)) {
    try {
      const { owner, repo } = normalizeOwnerRepo(params);
      if (owner && repo) {
        const commits = await fetchPRCommitsWithFiles(
          owner,
          repo,
          item.number,
          params
        );
        if (commits) {
          rawResponseChars += getRawResponseChars(commits) ?? 0;
          result.commits = commits;
        }
      }
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return attachRawResponseChars(result, rawResponseChars);
}

export async function transformPullRequestItemFromREST(
  item: PullRequestSimple | PullRequestItem,
  params: GitHubPullRequestsSearchParams,
  octokit: InstanceType<typeof OctokitWithThrottling>,
  authInfo?: AuthInfo
): Promise<GitHubPullRequestItem> {
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(item);

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  let rawResponseChars = 0;
  const owner = params.owner as string;
  const repo = params.repo as string;

  result.additions = 'additions' in item ? (item.additions ?? 0) : 0;
  result.deletions = 'deletions' in item ? (item.deletions ?? 0) : 0;
  if (!shouldFetchFileChanges(params)) {
    result.file_changes = {
      total_count: 'changed_files' in item ? (item.changed_files ?? 0) : 0,
      files: [],
    };
  }

  if (shouldFetchFileChanges(params)) {
    try {
      const fileChanges = await fetchPRFileChangesAPI(
        owner,
        repo,
        item.number,
        authInfo
      );
      if (fileChanges) {
        rawResponseChars += getRawResponseChars(fileChanges) ?? 0;
        fileChanges.files = applyPartialContentFilter(
          fileChanges.files,
          params
        ) as DiffEntry[];
        result.file_changes = fileChanges;
      }
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (files): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  const wantDiscussionRest = shouldFetchDiscussionComments(params);
  const wantInlineRest = shouldFetchInlineComments(params);
  if (wantDiscussionRest || wantInlineRest) {
    const includeBots = shouldIncludeBotComments(params);
    const emptyRest = (): Promise<{
      comments: PRCommentItem[];
      note?: string;
    }> => Promise.resolve({ comments: attachRawResponseChars([], 0) });
    const [
      { comments: discussionComments, note: discussionNote },
      { comments: inlineComments, note: inlineNote },
    ] = await Promise.all([
      wantDiscussionRest
        ? fetchPRComments(octokit, owner, repo, item.number, includeBots)
        : emptyRest(),
      wantInlineRest
        ? fetchPRInlineComments(octokit, owner, repo, item.number, includeBots)
        : emptyRest(),
    ]);

    result.comments = [...discussionComments, ...inlineComments];
    rawResponseChars +=
      (getRawResponseChars(discussionComments) ?? 0) +
      (getRawResponseChars(inlineComments) ?? 0);

    const notes = [discussionNote, inlineNote].filter(
      (n): n is string => typeof n === 'string'
    );
    if (notes.length > 0) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        ...notes,
      ];
    }
  }

  if (shouldFetchReviews(params)) {
    try {
      const reviews = await fetchPRReviews(octokit, owner, repo, item.number);
      rawResponseChars += getRawResponseChars(reviews) ?? 0;
      result.reviews = reviews;
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch reviews: ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  if (shouldFetchCommits(params)) {
    try {
      const commits = await fetchPRCommitsWithFiles(
        owner,
        repo,
        item.number,
        params,
        authInfo
      );
      if (commits) {
        rawResponseChars += getRawResponseChars(commits) ?? 0;
        result.commits = commits;
      }
    } catch (error: unknown) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return attachRawResponseChars(result, rawResponseChars);
}
