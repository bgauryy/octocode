//! Complete-collection GraphQL fast path for a single PR history item.
import { getConfigSync } from '@octocodeai/config';
import type { OctokitWithThrottling } from './client.js';
import type {
  CommitInfo,
  GitHubPullRequestsSearchParams,
  PRCommentItem,
  PRReviewInfo,
} from './githubAPI.js';
import {
  shouldFetchCommits,
  shouldFetchDiscussionComments,
  shouldFetchFileChanges,
  shouldFetchReviews,
} from './prContentFetcher/flags.js';

type CollectionStatus = 'unused' | 'complete' | 'incomplete';

export type GraphqlPrCollections = {
  files: CollectionStatus;
  discussion: CollectionStatus;
  reviews: CollectionStatus;
  commits: CollectionStatus;
  mappedFiles?: Array<{
    filename: string;
    additions?: number;
    deletions?: number;
    status: string;
  }>;
  mappedComments?: PRCommentItem[];
  mappedReviews?: PRReviewInfo[];
  mappedCommits?: CommitInfo[];
};

const skippedHosts = new Set<string>();

export function graphqlIsSkipped(host: string): boolean {
  return skippedHosts.has(host);
}

export function skipGraphqlHost(host: string): void {
  skippedHosts.add(host);
}

export function graphqlCompleteCollectionEligible(
  params: GitHubPullRequestsSearchParams
): boolean {
  if (params.prNumber === undefined) return false;
  if (params.page !== undefined && params.page > 1) return false;
  const pages = params.collectionPages ?? {};
  if (
    (pages.changedFiles ?? 1) > 1 ||
    (pages.discussion ?? 1) > 1 ||
    (pages.inline ?? 1) > 1 ||
    (pages.reviews ?? 1) > 1 ||
    (pages.commits ?? 1) > 1
  ) {
    return false;
  }
  const patches = (
    params.content as { patches?: { mode?: string } } | undefined
  )?.patches?.mode;
  if (patches && patches !== 'none') return false;
  const includeCommitFiles = (
    params.content as { commits?: { includeFiles?: boolean } } | undefined
  )?.commits?.includeFiles;
  if (includeCommitFiles === true) return false;
  const flags = [
    Boolean((params.content as { body?: boolean } | undefined)?.body),
    shouldFetchFileChanges(params),
    shouldFetchDiscussionComments(params),
    shouldFetchCommits(params),
    shouldFetchReviews(params),
  ].filter(Boolean).length;
  return flags >= 2;
}

function connectionState(
  node: Record<string, unknown> | undefined,
  wanted: boolean
): CollectionStatus {
  if (!wanted) return 'unused';
  if (!node) return 'incomplete';
  const pageInfo = node.pageInfo as { hasNextPage?: boolean } | undefined;
  return pageInfo?.hasNextPage === true ? 'incomplete' : 'complete';
}

export async function fetchGraphqlPullRequestCollections(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  params: GitHubPullRequestsSearchParams
): Promise<GraphqlPrCollections | null> {
  let graphqlEnabled: boolean;
  try {
    graphqlEnabled = getConfigSync().github.graphqlEnabled !== false;
  } catch {
    graphqlEnabled = true;
  }
  let credentialHost: string;
  try {
    const api = getConfigSync().github.apiUrl || 'https://api.github.com';
    const host = new URL(api).host;
    credentialHost = host === 'api.github.com' ? 'github.com' : host;
  } catch {
    credentialHost = 'github.com';
  }
  if (!graphqlEnabled || graphqlIsSkipped(credentialHost)) return null;
  if (!graphqlCompleteCollectionEligible(params)) return null;
  const owner = String(params.owner);
  const repo = String(params.repo);
  const number = params.prNumber as number;
  const wantFiles = shouldFetchFileChanges(params);
  const wantDiscussion = shouldFetchDiscussionComments(params);
  const wantReviews = shouldFetchReviews(params);
  const wantCommits = shouldFetchCommits(params);
  const selections = [
    'number title url state body isDraft isMerged author { login }',
  ];
  const variables: Record<string, unknown> = { owner, repo, number };
  if (wantFiles) {
    selections.push(
      'files(first:$files){ pageInfo{ hasNextPage } nodes{ path additions deletions changeType } }'
    );
    variables.files = 100;
  }
  if (wantDiscussion) {
    selections.push(
      'commentsConn: comments(first:$discussion){ pageInfo{ hasNextPage } nodes{ databaseId author{ login } body createdAt updatedAt } }'
    );
    variables.discussion = 100;
  }
  if (wantReviews) {
    selections.push(
      'reviews(first:$reviews){ pageInfo{ hasNextPage } nodes{ id author{ login } state body submittedAt commit{ oid } } }'
    );
    variables.reviews = 100;
  }
  if (wantCommits) {
    selections.push(
      'commits(first:$commits){ pageInfo{ hasNextPage } nodes{ commit{ oid messageHeadline authoredDate author{ user{ login } } } } }'
    );
    variables.commits = 50;
  }
  const args = [
    '$owner:String!',
    '$repo:String!',
    '$number:Int!',
    wantFiles ? '$files:Int!' : null,
    wantDiscussion ? '$discussion:Int!' : null,
    wantReviews ? '$reviews:Int!' : null,
    wantCommits ? '$commits:Int!' : null,
  ]
    .filter(Boolean)
    .join(',');
  const document = `query(${args}){ repository(owner:$owner,name:$repo){ pullRequest(number:$number){ ${selections.join(' ')} } } }`;
  try {
    const page = (await octokit.graphql(document, variables)) as {
      repository?: { pullRequest?: Record<string, unknown> | null };
    };
    const pr = page.repository?.pullRequest;
    if (!pr) return null;
    const files = connectionState(
      pr.files as Record<string, unknown> | undefined,
      wantFiles
    );
    const discussion = connectionState(
      pr.commentsConn as Record<string, unknown> | undefined,
      wantDiscussion
    );
    const reviews = connectionState(
      pr.reviews as Record<string, unknown> | undefined,
      wantReviews
    );
    const commits = connectionState(
      pr.commits as Record<string, unknown> | undefined,
      wantCommits
    );
    return {
      files,
      discussion,
      reviews,
      commits,
      mappedFiles:
        files === 'complete'
          ? (
              (pr.files as { nodes?: Array<Record<string, unknown>> })?.nodes ??
              []
            ).map(node => ({
              filename: String(node.path ?? ''),
              additions: node.additions as number | undefined,
              deletions: node.deletions as number | undefined,
              status:
                node.changeType === 'ADDED'
                  ? 'added'
                  : node.changeType === 'DELETED'
                    ? 'removed'
                    : node.changeType === 'RENAMED'
                      ? 'renamed'
                      : 'modified',
            }))
          : undefined,
      mappedComments:
        discussion === 'complete'
          ? (
              (
                pr.commentsConn as {
                  nodes?: Array<Record<string, unknown>>;
                }
              )?.nodes ?? []
            ).map(node => {
              const createdAt = String(node.createdAt ?? '');
              return {
                id: String(node.databaseId ?? node.id ?? ''),
                body: String(node.body ?? ''),
                user: String(
                  (node.author as { login?: string } | undefined)?.login ??
                    'unknown'
                ),
                createdAt,
                updatedAt: String(node.updatedAt ?? createdAt),
                commentType: 'discussion' as const,
              };
            })
          : undefined,
      mappedReviews:
        reviews === 'complete'
          ? (
              (pr.reviews as { nodes?: Array<Record<string, unknown>> })
                ?.nodes ?? []
            ).map(node => ({
              id: String(node.id ?? ''),
              user: String(
                (node.author as { login?: string } | undefined)?.login ??
                  'unknown'
              ),
              state: String(node.state ?? ''),
              body: String(node.body ?? ''),
              submittedAt:
                typeof node.submittedAt === 'string'
                  ? node.submittedAt
                  : undefined,
              commitId:
                typeof (node.commit as { oid?: unknown } | undefined)?.oid ===
                'string'
                  ? (node.commit as { oid: string }).oid
                  : undefined,
            }))
          : undefined,
      mappedCommits:
        commits === 'complete'
          ? (
              (pr.commits as { nodes?: Array<Record<string, unknown>> })
                ?.nodes ?? []
            ).map(node => {
              const commit = (node.commit ?? {}) as Record<string, unknown>;
              const author = (commit.author ?? {}) as Record<string, unknown>;
              const user = (author.user ?? {}) as Record<string, unknown>;
              return {
                sha: String(commit.oid ?? ''),
                message: String(commit.messageHeadline ?? ''),
                author: String(user.login ?? 'unknown'),
                date: String(commit.authoredDate ?? ''),
                files: [],
              };
            })
          : undefined,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (
      message.includes('RATE_LIMITED') ||
      message.includes('API rate limit')
    ) {
      skipGraphqlHost(credentialHost);
    }
    return null;
  }
}
