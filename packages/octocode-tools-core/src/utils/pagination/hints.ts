import type {
  PaginationMetadata,
  GeneratePaginationHintsOptions,
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
    const startChar = metadata.charOffset + 1;
    const endChar = metadata.charOffset + metadata.charLength;
    return [
      `Page ${metadata.currentPage}/${metadata.totalPages} (chars ${startChar}-${endChar} of ${metadata.totalChars}). Next: charOffset=${metadata.nextCharOffset}`,
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

export function generateStructurePaginationHints(
  pagination: StructurePaginationInfo,
  _context: StructurePaginationHintContext
): string[] {
  if (!pagination.hasMore) return [];

  return [
    `Page ${pagination.currentPage}/${pagination.totalPages}. Next: page=${pagination.currentPage + 1}`,
  ];
}
