import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PRCommentItem,
  CommitInfo,
  PRReviewInfo,
  DiffEntry,
  CommitFileInfo,
  IssueSearchResultItem,
  PullRequestSimple,
  PullRequestItem,
  IssueComment,
} from './githubAPI.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { logSessionError } from '../session.js';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { contextUtils } from '../utils/contextUtils.js';
import { getOctokit, OctokitWithThrottling } from './client.js';
import { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types';
import {
  createBasePRTransformation,
  normalizeOwnerRepo,
  applyPartialContentFilter,
} from './prTransformation.js';
import {
  attachRawResponseChars,
  countSerializedChars,
  getRawResponseChars,
} from '../utils/response/charSavings.js';

function shouldFetchFileChanges(
  params: GitHubPullRequestsSearchParams
): boolean {
  const content = params.content as
    | {
        changedFiles?: boolean;
        patches?: { mode?: 'none' | 'selected' | 'all' };
      }
    | undefined;
  return Boolean(
    params.reviewMode === 'full' ||
    content?.changedFiles ||
    (content?.patches?.mode && content.patches.mode !== 'none')
  );
}

type ContentComments = {
  discussion?: boolean;
  reviewInline?: boolean;
  includeBots?: boolean;
  file?: string;
};

function getCommentsConfig(
  params: GitHubPullRequestsSearchParams
): ContentComments | null {
  if (params.reviewMode === 'full')
    return { discussion: true, reviewInline: true };
  const c = (params.content as { comments?: ContentComments } | undefined)
    ?.comments;
  return c ?? null;
}

function shouldFetchDiscussionComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const cfg = getCommentsConfig(params);
  if (!cfg) return false;
  return cfg.discussion !== false;
}

function shouldFetchInlineComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const cfg = getCommentsConfig(params);
  if (!cfg) return false;
  return cfg.reviewInline !== false;
}

function shouldFetchCommits(params: GitHubPullRequestsSearchParams): boolean {
  const content = params.content as { commits?: unknown } | undefined;
  return Boolean(params.reviewMode === 'full' || content?.commits);
}

function shouldFetchReviews(params: GitHubPullRequestsSearchParams): boolean {
  const content = params.content as { reviews?: boolean } | undefined;
  return Boolean(params.reviewMode === 'full' || content?.reviews);
}

function shouldIncludeBotComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const content = params.content as
    | { comments?: { includeBots?: boolean } }
    | undefined;
  return Boolean(content?.comments?.includeBots);
}

const KNOWN_BOT_LOGINS = new Set([
  'vercel',
  'pkg-pr-new',
  'coderabbitai',
  'github-actions',
  'codecov',
  'changeset-bot',
  'netlify',
  'sonarcloud',
  'socket-security',
]);

function isBotAuthor(login: string): boolean {
  const l = login.toLowerCase();
  return l.endsWith('[bot]') || KNOWN_BOT_LOGINS.has(l.replace(/\[bot\]$/, ''));
}

function stripMachineBlobs(body: string): string {
  return body
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/^\[vc\]:\s*#.*$/gm, '')
    .replace(/^[A-Za-z0-9+/]{120,}={0,2}$/gm, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

async function fetchPRComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false
): Promise<{ comments: PRCommentItem[]; note?: string }> {
  try {
    const raw: IssueComment[] = [];
    let rawResponseChars = 0;
    let page = 1;
    let keepFetching = true;
    do {
      const commentsResult = await octokit.rest.issues.listComments({
        owner,
        repo,
        issue_number: prNumber,
        per_page: 100,
        page,
      });
      rawResponseChars += countSerializedChars(commentsResult.data);
      raw.push(...commentsResult.data);
      keepFetching = commentsResult.data.length === 100;
      page++;
    } while (keepFetching);

    const kept = includeBots
      ? raw
      : raw.filter((c: IssueComment) => !isBotAuthor(c.user?.login ?? ''));
    const botsDropped = raw.length - kept.length;

    const comments = kept.map((comment: IssueComment): PRCommentItem => {
      const stripped = contextUtils.minifyMarkdownCore(
        stripMachineBlobs(comment.body ?? '')
      );
      return {
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(stripped).content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
        commentType: 'discussion',
      };
    });

    const notes: string[] = [];
    if (botsDropped > 0) {
      notes.push(
        `${botsDropped} bot comment(s) hidden (set content.comments.includeBots:true to include)`
      );
    }

    return {
      comments: attachRawResponseChars(comments, rawResponseChars),
      note: notes.length > 0 ? notes.join('; ') : undefined,
    };
  } catch {
    return { comments: attachRawResponseChars([], 0) };
  }
}

async function fetchPRReviews(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number
): Promise<PRReviewInfo[]> {
  try {
    const { items, rawResponseChars } = await fetchAllPaginated<
      Awaited<ReturnType<typeof octokit.rest.pulls.listReviews>>['data'][number]
    >(
      page =>
        octokit.rest.pulls.listReviews({
          owner,
          repo,
          pull_number: prNumber,
          per_page: 100,
          page,
        }) as Promise<{
          data: Awaited<
            ReturnType<typeof octokit.rest.pulls.listReviews>
          >['data'];
        }>
    );

    return attachRawResponseChars(
      items.map(review => ({
        id: String(review.id),
        user: review.user?.login ?? 'unknown',
        state: review.state ?? '',
        body: ContentSanitizer.sanitizeContent(
          contextUtils.minifyMarkdownCore(stripMachineBlobs(review.body ?? ''))
        ).content,
        submitted_at: review.submitted_at ?? undefined,
        commit_id: review.commit_id ?? undefined,
      })),
      rawResponseChars
    );
  } catch {
    return attachRawResponseChars([], 0);
  }
}

async function fetchPRInlineComments(
  octokit: InstanceType<typeof OctokitWithThrottling>,
  owner: string,
  repo: string,
  prNumber: number,
  includeBots: boolean = false
): Promise<{ comments: PRCommentItem[]; note?: string }> {
  try {
    type ReviewComment = Awaited<
      ReturnType<typeof octokit.rest.pulls.listReviewComments>
    >['data'][number];

    const raw: ReviewComment[] = [];
    let rawResponseChars = 0;
    let page = 1;
    let keepFetching = true;
    do {
      const result = await octokit.rest.pulls.listReviewComments({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      });
      rawResponseChars += countSerializedChars(result.data);
      raw.push(...result.data);
      keepFetching = result.data.length === 100;
      page++;
    } while (keepFetching);

    const kept = includeBots
      ? raw
      : raw.filter((c: ReviewComment) => !isBotAuthor(c.user?.login ?? ''));
    const botsDropped = raw.length - kept.length;

    const comments = kept.map((comment: ReviewComment): PRCommentItem => {
      const stripped = contextUtils.minifyMarkdownCore(
        stripMachineBlobs(comment.body ?? '')
      );
      return {
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(stripped).content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
        commentType: 'review_inline',
        path: comment.path,
        line: comment.line ?? comment.original_line ?? undefined,
        ...(comment.in_reply_to_id != null
          ? { in_reply_to_id: comment.in_reply_to_id }
          : {}),
      };
    });

    const notes: string[] = [];
    if (botsDropped > 0) {
      notes.push(
        `${botsDropped} bot inline comment(s) hidden (set content.comments.includeBots:true to include)`
      );
    }

    return {
      comments: attachRawResponseChars(comments, rawResponseChars),
      note: notes.length > 0 ? notes.join('; ') : undefined,
    };
  } catch {
    return { comments: attachRawResponseChars([], 0) };
  }
}

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

  if (item.pull_request) {
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return attachRawResponseChars(result, rawResponseChars);
}

async function fetchAllPaginated<T>(
  fetchPage: (page: number) => Promise<{ data: T[] }>
): Promise<{ items: T[]; rawResponseChars: number }> {
  const items: T[] = [];
  let rawResponseChars = 0;
  let page = 1;
  let keepFetching = true;

  do {
    const result = await fetchPage(page);
    rawResponseChars += countSerializedChars(result.data);
    items.push(...result.data);
    keepFetching = result.data.length === 100;
    page++;
  } while (keepFetching);

  return { items, rawResponseChars };
}

async function fetchPRFileChangesAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<{ total_count: number; files: DiffEntry[] } | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars } = await fetchAllPaginated<DiffEntry>(
    page =>
      octokit.rest.pulls.listFiles({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      }) as Promise<{ data: DiffEntry[] }>
  );

  return attachRawResponseChars(
    {
      total_count: items.length,
      files: items,
    },
    rawResponseChars
  );
}

interface CommitListItem {
  sha: string;
  commit: {
    message: string;
    author: {
      name: string;
      date: string;
    } | null;
  };
}

async function fetchPRCommitsAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<CommitListItem[] | null> {
  const octokit = await getOctokit(authInfo);
  const { items, rawResponseChars } = await fetchAllPaginated<CommitListItem>(
    page =>
      octokit.rest.pulls.listCommits({
        owner,
        repo,
        pull_number: prNumber,
        per_page: 100,
        page,
      }) as Promise<{ data: CommitListItem[] }>
  );

  return attachRawResponseChars(items, rawResponseChars);
}

async function fetchCommitFilesAPI(
  owner: string,
  repo: string,
  sha: string,
  authInfo?: AuthInfo
): Promise<CommitFileInfo[] | null> {
  try {
    const octokit = await getOctokit(authInfo);
    const result = await octokit.rest.repos.getCommit({
      owner,
      repo,
      ref: sha,
    });

    return attachRawResponseChars(
      (result.data.files || []) as CommitFileInfo[],
      result.data
    );
  } catch {
    return null;
  }
}

async function fetchPRCommitsWithFiles(
  owner: string,
  repo: string,
  prNumber: number,
  params: GitHubPullRequestsSearchParams,
  authInfo?: AuthInfo
): Promise<CommitInfo[] | null> {
  const commits = await fetchPRCommitsAPI(owner, repo, prNumber, authInfo);
  if (!commits) return null;

  let rawResponseChars = getRawResponseChars(commits) ?? 0;
  const sortedCommits = [...commits].sort((a, b) => {
    const dateA = a.commit.author?.date
      ? new Date(a.commit.author.date).getTime()
      : 0;
    const dateB = b.commit.author?.date
      ? new Date(b.commit.author.date).getTime()
      : 0;
    return dateB - dateA;
  });

  const commitInfos: CommitInfo[] = await Promise.all(
    sortedCommits.map(async commit => {
      const files = await fetchCommitFilesAPI(
        owner,
        repo,
        commit.sha,
        authInfo
      );

      let processedFiles: CommitInfo['files'] = [];

      if (files) {
        rawResponseChars += getRawResponseChars(files) ?? 0;
        processedFiles = applyPartialContentFilter(
          files,
          params
        ) as CommitFileInfo[];
      }

      return {
        sha: commit.sha,
        message: commit.commit.message,
        author: commit.commit.author?.name || 'unknown',
        date: commit.commit.author?.date || '',
        files: processedFiles,
      };
    })
  );

  return attachRawResponseChars(commitInfos, rawResponseChars);
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
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
      logSessionError(TOOL_NAMES.GITHUB_SEARCH_PULL_REQUESTS, String(error));
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        `Partial Data: Failed to fetch details (commits): ${error instanceof Error ? error.message : String(error)}`,
      ];
    }
  }

  return attachRawResponseChars(result, rawResponseChars);
}
