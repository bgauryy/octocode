import type { GitHubRepositoryOutput } from '@octocodeai/octocode-core/extra-types';
import { compareIsoDateDescending } from '../../../utils/core/compare.js';
import type { PartialReposSearchQuery } from './queryVariants.js';

export function deduplicateRepositories(
  repositories: GitHubRepositoryOutput[]
): GitHubRepositoryOutput[] {
  const uniqueRepositories = new Map<string, GitHubRepositoryOutput>();

  for (const repo of repositories) {
    const key = `${repo.owner}/${repo.repo}`;
    if (!uniqueRepositories.has(key)) {
      uniqueRepositories.set(key, repo);
    }
  }

  return [...uniqueRepositories.values()];
}

export function rankRepositoriesByRelevance(
  repositories: readonly GitHubRepositoryOutput[],
  query: PartialReposSearchQuery
): GitHubRepositoryOutput[] {
  return [...repositories].sort((left, right) => {
    const requestedSort = compareByRequestedSort(left, right, query.sort);
    if (requestedSort !== 0) return requestedSort;

    const relevanceDelta =
      scoreRepositoryRelevance(right, query) -
      scoreRepositoryRelevance(left, query);
    if (relevanceDelta !== 0) return relevanceDelta;

    const starsDelta = (right.stars ?? 0) - (left.stars ?? 0);
    if (starsDelta !== 0) return starsDelta;

    return repositoryFullName(left).localeCompare(repositoryFullName(right));
  });
}

function compareByRequestedSort(
  left: GitHubRepositoryOutput,
  right: GitHubRepositoryOutput,
  sort: PartialReposSearchQuery['sort']
): number {
  switch (sort) {
    case 'stars':
      return (right.stars ?? 0) - (left.stars ?? 0);
    case 'forks':
      return (right.forksCount ?? 0) - (left.forksCount ?? 0);
    case 'help-wanted-issues':
      return (right.openIssuesCount ?? 0) - (left.openIssuesCount ?? 0);
    case 'updated':
      return compareIsoDateDescending(left.updatedAt, right.updatedAt);
    case 'best-match':
    case undefined:
      return 0;
    default:
      return 0;
  }
}

function scoreRepositoryRelevance(
  repo: GitHubRepositoryOutput,
  query: PartialReposSearchQuery
): number {
  const terms = getRepositorySearchTerms(query);
  const fullName = repositoryFullName(repo).toLowerCase();
  const repoName = repo.repo.toLowerCase();
  const description = (repo.description ?? '').toLowerCase();
  const topics = (repo.topics ?? []).map(topic => topic.toLowerCase());
  const language = repo.language?.toLowerCase();
  const requestedLanguage = query.language?.toLowerCase();

  const termScore = terms.reduce((score, term) => {
    if (repoName === term || fullName === term) return score + 80;
    if (repoName.includes(term) || fullName.includes(term)) return score + 40;
    if (topics.includes(term)) return score + 35;
    if (description.includes(term)) return score + 10;
    return score;
  }, 0);

  return (
    termScore + (requestedLanguage && language === requestedLanguage ? 20 : 0)
  );
}

function getRepositorySearchTerms(
  query: PartialReposSearchQuery
): readonly string[] {
  const keywords = query.keywords ?? [];
  const topics = query.topicsToSearch ?? [];
  return [...keywords, ...topics]
    .map(term => term.trim().toLowerCase())
    .filter(term => term.length > 0);
}

function repositoryFullName(repo: GitHubRepositoryOutput): string {
  return `${repo.owner}/${repo.repo}`;
}
