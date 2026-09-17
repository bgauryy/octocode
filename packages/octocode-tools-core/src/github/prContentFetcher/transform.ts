import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PRCommentItem,
  DiffEntry,
  PullRequestSimple,
  PullRequestItem,
} from '../githubAPI.js';
import { OctokitWithThrottling } from '../client.js';
import { AuthInfo } from '@modelcontextprotocol/server';
import {
  createBasePRTransformation,
  applyPartialContentFilter,
} from '../prTransformation.js';
import {
  attachRawResponseChars,
  getRawResponseChars,
} from '../../utils/response/charSavings.js';
import {
  shouldFetchFileChanges,
  shouldFetchDiscussionComments,
  shouldFetchInlineComments,
  shouldFetchCommits,
  shouldFetchReviews,
  shouldIncludeBotComments,
} from './flags.js';
import {
  fetchPRComments,
  fetchPRInlineComments,
  fetchPRReviews,
} from './comments.js';
import type { CollectionArray } from './collectionPaging.js';
import { fetchPRFileChangesAPI, fetchPRCommitsWithFiles } from './commits.js';
import { fetchGraphqlPullRequestCollections } from '../graphqlHistory.js';

export { transformPullRequestItemFromSearch } from './transformSearch.js';

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

  const graphql = await fetchGraphqlPullRequestCollections(octokit, params);

  if (shouldFetchFileChanges(params)) {
    if (graphql?.files === 'complete' && graphql.mappedFiles) {
      result.file_changes = {
        total_count:
          'changed_files' in item
            ? (item.changed_files ?? graphql.mappedFiles.length)
            : graphql.mappedFiles.length,
        files: applyPartialContentFilter(
          graphql.mappedFiles as DiffEntry[],
          params
        ) as DiffEntry[],
      };
      result.collectionStates.changedFiles = { page: 1, hasMore: false };
    } else {
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
  }

  const wantDiscussionRest = shouldFetchDiscussionComments(params);
  const wantInlineRest = shouldFetchInlineComments(params);
  if (wantDiscussionRest || wantInlineRest) {
    const includeBots = shouldIncludeBotComments(params);
    type CommentFetchResult = {
      comments: CollectionArray<PRCommentItem>;
      note?: string;
    };
    const emptyRest = (): Promise<CommentFetchResult> =>
      Promise.resolve({ comments: attachRawResponseChars([], 0) });
    const completeGraphqlComments = (
      comments: PRCommentItem[]
    ): Promise<CommentFetchResult> =>
      Promise.resolve({
        comments: Object.assign([...comments], {
          collectionState: { page: 1, hasMore: false },
        }),
      });
    const [
      { comments: discussionComments, note: discussionNote },
      { comments: inlineComments, note: inlineNote },
    ] = await Promise.all([
      wantDiscussionRest
        ? graphql?.discussion === 'complete' && graphql.mappedComments
          ? completeGraphqlComments(graphql.mappedComments)
          : fetchPRComments(
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
    if (graphql?.reviews === 'complete' && graphql.mappedReviews) {
      result.reviews = Object.assign([...graphql.mappedReviews], {
        collectionState: { page: 1, hasMore: false },
      });
      result.collectionStates.reviews = { page: 1, hasMore: false };
    } else {
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
    if (graphql?.commits === 'complete' && graphql.mappedCommits) {
      result.commits = Object.assign([...graphql.mappedCommits], {
        collectionState: { page: 1, hasMore: false },
      });
      result.collectionStates.commits = { page: 1, hasMore: false };
    } else {
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
