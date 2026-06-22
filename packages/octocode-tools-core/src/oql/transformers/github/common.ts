import type { OqlQuery, QueryScope, QuerySource } from '../../types.js';

export function splitGithubSource(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (source?.kind !== 'github') return {};
  if (source.repo?.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return {
    ...(source.owner ? { owner: source.owner } : {}),
    ...(source.repo ? { repo: source.repo } : {}),
  };
}

export function firstScopePath(
  scope: QueryScope | undefined
): string | undefined {
  if (!scope?.path) return undefined;
  return Array.isArray(scope.path) ? scope.path[0] : scope.path;
}

export function firstScopeLanguage(
  scope: QueryScope | undefined
): string | undefined {
  if (!scope?.language) return undefined;
  return Array.isArray(scope.language) ? scope.language[0] : scope.language;
}

export function requestedRowLimit(query: OqlQuery): number | undefined {
  return typeof query.limit === 'number'
    ? query.limit
    : typeof query.itemsPerPage === 'number'
      ? query.itemsPerPage
      : undefined;
}
