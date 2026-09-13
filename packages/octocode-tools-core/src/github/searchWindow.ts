import type { GitHubAPIError } from './githubAPI.js';

export const GITHUB_SEARCH_RESULT_WINDOW = 1000;

export function rejectUnreachableSearchPage(
  page: number,
  perPage: number
): GitHubAPIError | undefined {
  if ((page - 1) * perPage < GITHUB_SEARCH_RESULT_WINDOW) return undefined;
  return {
    error:
      'GitHub search exposes at most 1000 matches. Narrow the query or date bounds before requesting this page.',
    type: 'http',
    status: 400,
  };
}
