/**
 * PR content fetching — comments, commits, file changes, and item transformation.
 * Extracted from pullRequestSearch.ts.
 */
import {
  GitHubPullRequestsSearchParams,
  GitHubPullRequestItem,
  PRCommentItem,
  CommitInfo,
  DiffEntry,
  CommitFileInfo,
  IssueSearchResultItem,
  PullRequestSimple,
  PullRequestItem,
  IssueComment,
} from './githubAPI.js';
import { TOOL_NAMES } from '../tools/toolMetadata/proxies.js';
import { logSessionError } from '../session.js';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
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

// PR threads on popular repos are dominated by automation: deploy-preview
// tables, install-this-PR blocks, and reviewer-bot status. None of it is
// load-bearing for "what does this PR do / why the disagreement", yet it can
// be the single largest cost in a metered run. Default to dropping it.
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

/**
 * Strip machine-generated blobs that bloat comment bodies without adding
 * review signal: HTML comment blocks (CodeRabbit `<!-- internal state … -->`),
 * Vercel `[vc]: #…` base64 markers, and other base64-only lines.
 */
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
    // Page through ALL comments (GitHub caps per_page at 100). Same loop the
    // file fetch uses below — never stop at page 1, so comments 101+ are not
    // silently dropped. Total size is then bounded losslessly by the response
    // char-paginator.
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

    // No count cap: keep EVERY non-bot comment. Oversized threads are bounded
    // losslessly by the response char-paginator (agents page for more via
    // responseCharOffset), never by silently dropping the tail.
    const comments = kept.map((comment: IssueComment): PRCommentItem => {
      const stripped = stripMachineBlobs(comment.body ?? '');
      return {
        id: String(comment.id),
        user: comment.user?.login ?? 'unknown',
        body: ContentSanitizer.sanitizeContent(stripped).content,
        created_at: comment.created_at ?? '',
        updated_at: comment.updated_at ?? '',
      };
    });

    const notes: string[] = [];
    if (botsDropped > 0) {
      notes.push(`${botsDropped} bot comment(s) hidden (set includeBots:true)`);
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
  // Cast to RawPRData - Search API items may have merged_at in extended response
  const rawItem = item as IssueSearchResultItem & { merged_at?: string | null };
  const { prData: result, sanitizationWarnings } =
    createBasePRTransformation(rawItem);

  if (sanitizationWarnings.size > 0) {
    result._sanitization_warnings = Array.from(sanitizationWarnings);
  }

  let rawResponseChars = 0;
  const type = params.type || 'metadata';
  const shouldFetchContent =
    type === 'fullContent' || type === 'partialContent' || type === 'metadata';

  if (shouldFetchContent || item.pull_request) {
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

          if (shouldFetchContent) {
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

  if (params.withComments) {
    const { owner, repo } = normalizeOwnerRepo(params);
    if (owner && repo) {
      const { comments, note } = await fetchPRComments(
        octokit,
        owner,
        repo,
        item.number,
        params.includeBots
      );
      result.comments = comments;
      rawResponseChars += getRawResponseChars(comments) ?? 0;
      if (note) {
        result._sanitization_warnings = [
          ...(result._sanitization_warnings || []),
          note,
        ];
      }
    }
  }

  if (params.withCommits) {
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

async function fetchPRFileChangesAPI(
  owner: string,
  repo: string,
  prNumber: number,
  authInfo?: AuthInfo
): Promise<{ total_count: number; files: DiffEntry[] } | null> {
  const octokit = await getOctokit(authInfo);
  const allFiles: DiffEntry[] = [];
  let rawResponseChars = 0;
  let page = 1;
  let keepFetching = true;

  do {
    const result = await octokit.rest.pulls.listFiles({
      owner,
      repo,
      pull_number: prNumber,
      per_page: 100,
      page: page,
    });

    rawResponseChars += countSerializedChars(result.data);
    allFiles.push(...result.data);
    keepFetching = result.data.length === 100;
    page++;
  } while (keepFetching);

  return attachRawResponseChars(
    {
      total_count: allFiles.length,
      files: allFiles,
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
  const result = await octokit.rest.pulls.listCommits({
    owner,
    repo,
    pull_number: prNumber,
  });

  return attachRawResponseChars(result.data as CommitListItem[], result.data);
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
  const type = params.type || 'metadata';
  const shouldFetchContent =
    type === 'fullContent' || type === 'partialContent' || type === 'metadata';

  // Owner and repo are guaranteed to be strings for REST API calls
  const owner = params.owner as string;
  const repo = params.repo as string;

  if (shouldFetchContent) {
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

  if (params.withComments) {
    const { comments, note } = await fetchPRComments(
      octokit,
      owner,
      repo,
      item.number,
      params.includeBots
    );
    result.comments = comments;
    rawResponseChars += getRawResponseChars(comments) ?? 0;
    if (note) {
      result._sanitization_warnings = [
        ...(result._sanitization_warnings || []),
        note,
      ];
    }
  }

  if (params.withCommits) {
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
