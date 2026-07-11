import { countSerializedChars } from '../../../utils/response/charSavings.js';
import type {
  RepoSearchVariantExecution,
  RepoSearchVariantLabel,
  SuccessfulRepoSearchVariant,
} from './queryVariants.js';

export function buildResultPagination(pagination: {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
}) {
  return {
    currentPage: pagination.currentPage,
    totalPages: pagination.totalPages,
    perPage: pagination.entriesPerPage ?? 10,
    ...(pagination.totalMatches !== undefined
      ? { totalMatches: pagination.totalMatches }
      : {}),
    ...(pagination.reportedTotalMatches !== undefined
      ? { reportedTotalMatches: pagination.reportedTotalMatches }
      : {}),
    ...(pagination.reachableTotalMatches !== undefined
      ? { reachableTotalMatches: pagination.reachableTotalMatches }
      : {}),
    ...(pagination.totalMatchesKind !== undefined
      ? { totalMatchesKind: pagination.totalMatchesKind }
      : {}),
    ...(pagination.totalMatchesCapped !== undefined
      ? { totalMatchesCapped: pagination.totalMatchesCapped }
      : {}),
    hasMore: pagination.hasMore,
    ...(pagination.hasMore ? { nextPage: pagination.currentPage + 1 } : {}),
  };
}

export type EffectivePagination = {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;
  entriesPerPage?: number;
  totalMatches?: number;
  reportedTotalMatches?: number;
  reachableTotalMatches?: number;
  totalMatchesKind?: 'exact' | 'reported' | 'lowerBound';
  totalMatchesCapped?: boolean;
};

export function buildPartialFailureWarnings(
  failedVariants: readonly { label: RepoSearchVariantLabel }[]
): string[] | undefined {
  if (failedVariants.length === 0) return undefined;
  const labels = failedVariants.map(variant => `'${variant.label}'`).join(', ');
  return [
    `Repository search partially failed: the ${labels} query variant(s) returned an error. Results may be incomplete — retry or narrow the query.`,
  ];
}

export function buildMergedPagination(
  variants: SuccessfulRepoSearchVariant[],
  dedupedCount: number
): EffectivePagination | undefined {
  const pages = variants
    .map(variant => variant.response.data.pagination)
    .filter((p): p is NonNullable<typeof p> => Boolean(p));
  if (pages.length === 0) return undefined;

  // The variants are deduplicated before this point, so summing per-variant
  // totals overcounts every repository that appeared in more than one variant.
  // The count of distinct repositories we actually merged is a firm lower
  // bound on the true number of matches; report it as such.
  return {
    currentPage: pages[0]!.currentPage,
    totalPages: Math.max(...pages.map(p => p.totalPages)),
    hasMore: pages.some(p => p.hasMore),
    entriesPerPage: pages[0]!.entriesPerPage,
    totalMatches: dedupedCount,
    reachableTotalMatches: dedupedCount,
    totalMatchesKind: 'lowerBound',
    totalMatchesCapped: pages.some(p => p.totalMatchesCapped === true),
  };
}

export function sumVariantRawResponseChars(
  variants: RepoSearchVariantExecution[]
): number {
  return variants.reduce(
    (sum, variant) =>
      sum +
      (variant.response.rawResponseChars ??
        countSerializedChars(variant.response.data ?? variant.response)),
    0
  );
}
