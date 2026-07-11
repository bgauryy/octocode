/**
 * Small helpers shared across the providerMappers/* domain files.
 * Kept internal to the providerMappers module (not part of its public
 * barrel contract) — imported directly by sibling files in this directory.
 */

export function toProviderProjectId(
  owner?: string,
  repo?: string
): string | undefined {
  return owner && repo ? `${owner}/${repo}` : undefined;
}

export function splitRepositoryPath(repositoryPath: string): {
  owner: string;
  repo: string;
} {
  const slashIdx = repositoryPath.lastIndexOf('/');
  if (slashIdx <= 0) {
    return { owner: '', repo: repositoryPath };
  }
  return {
    owner: repositoryPath.substring(0, slashIdx),
    repo: repositoryPath.substring(slashIdx + 1),
  };
}

export function countMetadata(
  pagination:
    | {
        reportedTotalMatches?: number;
        reachableTotalMatches?: number;
        totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
        totalMatchesCapped?: boolean;
        uniqueFileCount?: number;
      }
    | undefined
) {
  return {
    ...(typeof pagination?.reportedTotalMatches === 'number'
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(typeof pagination?.reachableTotalMatches === 'number'
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination?.totalMatchesKind
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(typeof pagination?.totalMatchesCapped === 'boolean'
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    ...(typeof pagination?.uniqueFileCount === 'number'
      ? { uniqueFileCount: pagination.uniqueFileCount }
      : {}),
  };
}
