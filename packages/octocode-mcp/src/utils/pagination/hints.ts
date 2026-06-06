import type { PaginationInfo } from '../../types/toolResults.js';
import type {
  PaginationMetadata,
  GeneratePaginationHintsOptions,
  GitHubFileContentHintContext,
  StructurePaginationInfo,
  StructurePaginationHintContext,
} from './types.js';

function generateTokenWarnings(
  estimatedTokens: number,
  enableWarnings: boolean
): string[] {
  if (!enableWarnings) return [];

  if (estimatedTokens > 50000) {
    return [
      `Response ~${estimatedTokens.toLocaleString()} tokens — exceeds typical context. Reduce charLength or refine the query.`,
    ];
  }
  if (estimatedTokens > 30000) {
    return [
      `Response ~${estimatedTokens.toLocaleString()} tokens — approaching context limit. Consider reducing charLength.`,
    ];
  }
  return [];
}

function generateNavigationHints(metadata: PaginationMetadata): string[] {
  if (metadata.hasMore && metadata.nextCharOffset !== undefined) {
    return [
      `Page ${metadata.currentPage}/${metadata.totalPages}. Next: charOffset=${metadata.nextCharOffset}`,
    ];
  }
  return [];
}

export function generatePaginationHints(
  metadata: PaginationMetadata,
  options: GeneratePaginationHintsOptions = {}
): string[] {
  const { enableWarnings = true, customHints = [] } = options;
  const hints: string[] = [];

  hints.push(...customHints);

  if (metadata.estimatedTokens) {
    hints.push(
      ...generateTokenWarnings(metadata.estimatedTokens, enableWarnings)
    );
  }

  hints.push(...generateNavigationHints(metadata));

  return hints;
}

export function generateGitHubPaginationHints(
  pagination: PaginationInfo,
  _query: GitHubFileContentHintContext
): string[] {
  if (!pagination.hasMore) return [];

  const nextOffset =
    (pagination.byteOffset ?? 0) + (pagination.byteLength ?? 0);

  return [
    `Page ${pagination.currentPage}/${pagination.totalPages}. Next: charOffset=${nextOffset}`,
  ];
}

export function generateStructurePaginationHints(
  pagination: StructurePaginationInfo,
  _context: StructurePaginationHintContext
): string[] {
  if (!pagination.hasMore) return [];

  return [
    `Page ${pagination.currentPage}/${pagination.totalPages}. Next: entryPageNumber=${pagination.currentPage + 1}`,
  ];
}
