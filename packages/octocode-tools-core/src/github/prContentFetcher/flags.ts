import { GitHubPullRequestsSearchParams } from '../githubAPI.js';

export function shouldFetchFileChanges(
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

export type ContentComments = {
  discussion?: boolean;
  reviewInline?: boolean;
  includeBots?: boolean;
  file?: string;
};

export function getCommentsConfig(
  params: GitHubPullRequestsSearchParams
): ContentComments | null {
  if (params.reviewMode === 'full')
    return { discussion: true, reviewInline: true };
  const c = (params.content as { comments?: ContentComments } | undefined)
    ?.comments;
  return c ?? null;
}

export function shouldFetchDiscussionComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const cfg = getCommentsConfig(params);
  if (!cfg) return false;
  return cfg.discussion !== false;
}

export function shouldFetchInlineComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const cfg = getCommentsConfig(params);
  if (!cfg) return false;
  return cfg.reviewInline !== false;
}

export function shouldFetchCommits(
  params: GitHubPullRequestsSearchParams
): boolean {
  const content = params.content as { commits?: unknown } | undefined;
  return Boolean(params.reviewMode === 'full' || content?.commits);
}

export function shouldFetchReviews(
  params: GitHubPullRequestsSearchParams
): boolean {
  const content = params.content as { reviews?: boolean } | undefined;
  return Boolean(params.reviewMode === 'full' || content?.reviews);
}

/**
 * Whether search hits need a per-PR `pulls.get` enrichment call.
 * Lean list searches skip it (~1 REST call per hit); detail/content/review
 * modes still enrich.
 */
export function shouldEnrichPullRequestFromSearch(
  params: GitHubPullRequestsSearchParams
): boolean {
  if (params.prNumber !== undefined) return true;
  if (params.reviewMode === 'full') return true;
  return (
    shouldFetchFileChanges(params) ||
    shouldFetchDiscussionComments(params) ||
    shouldFetchInlineComments(params) ||
    shouldFetchCommits(params) ||
    shouldFetchReviews(params)
  );
}

export function shouldIncludeBotComments(
  params: GitHubPullRequestsSearchParams
): boolean {
  const content = params.content as
    { comments?: { includeBots?: boolean } } | undefined;
  return Boolean(content?.comments?.includeBots);
}

export function stripMachineBlobs(body: string): string {
  return body
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/^\[vc\]:\s*#.*$/gm, '')
    .replace(/^[A-Za-z0-9+/]{120,}={0,2}$/gm, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}
