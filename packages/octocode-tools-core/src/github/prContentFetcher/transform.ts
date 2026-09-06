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
import { AuthInfo } from '@modelcontextprotocol/server';
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
import type { CollectionArray } from './collectionPaging.js';
import { fetchPRFileChangesAPI, fetchPRCommitsWithFiles } from './commits.js';

export async function transformPullRequestItemFromSearch(
  item: IssueSearchResultItem,
  params: GitHubPullRequestsSearchParams,
  octokit: InstanceType<typeof OctokitWithThrottling>,
  authInfo?: AuthInfo
): Promise<GitHubPullRequestItem> {
  const rawItem = {
    ...item,
    merged_at:
      item.pull_request?.merged_at ??
      (item as IssueSearchResultItem & { merged_at?: string | null }).merged_at,
  };
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(rawItem);
  result.collectionStates = {};

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  let rawResponseChars = 0;

  if (item.pull_request && shouldEnrichPullRequestFromSearch(params)) {
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
            item.number,
            authInfo,
            params.collectionPages?.changedFiles ?? 1
          );

          if (fileChanges) {
            rawResponseChars += getRawResponseChars(fileChanges) ?? 0;
            fileChanges.files = applyPartialContentFilter(
              fileChanges.files,
              params
            ) as DiffEntry[];

            fileChanges.total_count =
              prDetails.data.changed_files ?? fileChanges.total_count;
            result.file_changes = fileChanges;
            result.collectionStates.changedFiles = fileChanges.collectionState;
            if (fileChanges.providerLimits)
              result.providerLimits = [
                ...(result.providerLimits ?? []),
                ...fileChanges.providerLimits,
              ];
          }
        }
      }
    }
  }

  const wantDiscussion = shouldFetchDiscussionComments(params);
  const wantInline = shouldFetchInlineComments(params);
  if (wantDiscussion || wantInline) {
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
      const includeBots = shouldIncludeBotComments(params);
      const empty = (): Promise<{
        comments: CollectionArray<PRCommentItem>;
        note?: string;
      }> => Promise.resolve({ comments: attachRawResponseChars([], 0) });
      const [
        { comments: discussionComments, note: discussionNote },
        { comments: inlineComments, note: inlineNote },
      ] = await Promise.all([
        wantDiscussion
          ? fetchPRComments(
              octokit,
              owner,
              repo,
              item.number,
              includeBots,
              authInfo,
              params.collectionPages?.discussion ?? 1
            )
          : empty(),
        wantInline
          ? fetchPRInlineComments(
              octokit,
              owner,
              repo,
              item.number,
              includeBots,
              authInfo,
              params.collectionPages?.inline ?? 1
            )
          : empty(),
      ]);

      result.comments = [...discussionComments, ...inlineComments];
      result.collectionStates.discussion = discussionComments.collectionState;
      result.collectionStates.inline = inlineComments.collectionState;
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
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
      const reviews = await fetchPRReviews(
        octokit,
        owner,
        repo,
        item.number,
        authInfo,
        params.collectionPages?.reviews ?? 1
      );
      rawResponseChars += getRawResponseChars(reviews) ?? 0;
      result.reviews = reviews;
      result.collectionStates.reviews = reviews.collectionState;
    }
  }

  if (shouldFetchCommits(params)) {
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
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
        result.collectionStates.commits = commits.collectionState;
        if (commits.providerLimits)
          result.providerLimits = [
            ...(result.providerLimits ?? []),
            ...commits.providerLimits,
          ];
      }
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
  result.collectionStates = {};

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
    const fileChanges = await fetchPRFileChangesAPI(
      owner,
      repo,
      item.number,
      authInfo,
      params.collectionPages?.changedFiles ?? 1
    );
    if (fileChanges) {
      rawResponseChars += getRawResponseChars(fileChanges) ?? 0;
      fileChanges.files = applyPartialContentFilter(
        fileChanges.files,
        params
      ) as DiffEntry[];
      fileChanges.total_count =
        'changed_files' in item
          ? (item.changed_files ?? fileChanges.total_count)
          : fileChanges.total_count;
      result.file_changes = fileChanges;
      result.collectionStates.changedFiles = fileChanges.collectionState;
      if (fileChanges.providerLimits)
        result.providerLimits = [
          ...(result.providerLimits ?? []),
          ...fileChanges.providerLimits,
        ];
    }
  }

  const wantDiscussionRest = shouldFetchDiscussionComments(params);
  const wantInlineRest = shouldFetchInlineComments(params);
  if (wantDiscussionRest || wantInlineRest) {
    const includeBots = shouldIncludeBotComments(params);
    const emptyRest = (): Promise<{
      comments: CollectionArray<PRCommentItem>;
      note?: string;
    }> => Promise.resolve({ comments: attachRawResponseChars([], 0) });
    const [
      { comments: discussionComments, note: discussionNote },
      { comments: inlineComments, note: inlineNote },
    ] = await Promise.all([
      wantDiscussionRest
        ? fetchPRComments(
            octokit,
            owner,
            repo,
            item.number,
            includeBots,
            authInfo,
            params.collectionPages?.discussion ?? 1
          )
        : emptyRest(),
      wantInlineRest
        ? fetchPRInlineComments(
            octokit,
            owner,
            repo,
            item.number,
            includeBots,
            authInfo,
            params.collectionPages?.inline ?? 1
          )
        : emptyRest(),
    ]);

    result.comments = [...discussionComments, ...inlineComments];
    result.collectionStates.discussion = discussionComments.collectionState;
    result.collectionStates.inline = inlineComments.collectionState;
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
    const reviews = await fetchPRReviews(
      octokit,
      owner,
      repo,
      item.number,
      authInfo,
      params.collectionPages?.reviews ?? 1
    );
    rawResponseChars += getRawResponseChars(reviews) ?? 0;
    result.reviews = reviews;
    result.collectionStates.reviews = reviews.collectionState;
  }

  if (shouldFetchCommits(params)) {
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
      result.collectionStates.commits = commits.collectionState;
      if (commits.providerLimits)
        result.providerLimits = [
          ...(result.providerLimits ?? []),
          ...commits.providerLimits,
        ];
    }
  }

  return attachRawResponseChars(result, rawResponseChars);
}
