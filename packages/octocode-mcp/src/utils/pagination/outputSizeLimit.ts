import { applyPagination, createPaginationInfo } from './core.js';
import { getOutputCharLimit } from './charLimit.js';
import type { PaginationInfo } from '../../types/toolResults.js';

interface OutputSizeLimitOptions {
  charOffset?: number;

  charLength?: number;

  maxOutputChars?: number;

  recommendedCharLength?: number;
}

interface OutputSizeLimitResult {
  content: string;

  wasLimited: boolean;

  pagination?: PaginationInfo;

  warnings: string[];

  paginationHints: string[];
}

export function applyOutputSizeLimit(
  content: string,
  options: OutputSizeLimitOptions
): OutputSizeLimitResult {
  const limit = getOutputCharLimit();
  const maxOutputChars = options.maxOutputChars ?? limit;
  const recommendedCharLength = options.recommendedCharLength ?? limit;

  const warnings: string[] = [];

  if (options.charLength !== undefined || options.charOffset !== undefined) {
    const effectiveCharLength = options.charLength ?? recommendedCharLength;
    const effectiveCharOffset = options.charOffset ?? 0;

    const paginationMetadata = applyPagination(
      content,
      effectiveCharOffset,
      effectiveCharLength
    );

    const pagination = createPaginationInfo(paginationMetadata);

    return {
      content: paginationMetadata.paginatedContent,
      wasLimited: true,
      pagination,
      warnings,
      paginationHints: generateOutputPaginationHints(pagination),
    };
  }

  if (content.length > maxOutputChars) {
    warnings.push(
      `Auto-paginated: Output (${content.length} chars) exceeds ${maxOutputChars} char limit. Use charOffset/charLength to navigate.`
    );

    const paginationMetadata = applyPagination(
      content,
      0,
      recommendedCharLength
    );

    const pagination = createPaginationInfo(paginationMetadata);

    return {
      content: paginationMetadata.paginatedContent,
      wasLimited: true,
      pagination,
      warnings,
      paginationHints: generateOutputPaginationHints(pagination),
    };
  }

  return {
    content,
    wasLimited: false,
    warnings,
    paginationHints: [],
  };
}

function generateOutputPaginationHints(pagination: PaginationInfo): string[] {
  const hints: string[] = [];

  if (pagination.hasMore) {
    hints.push(
      `Page ${pagination.currentPage}/${pagination.totalPages} (${pagination.charLength} of ${pagination.totalChars} chars)`
    );
    const nextOffset =
      (pagination.charOffset ?? 0) + (pagination.charLength ?? 0);
    hints.push(`Next page: use charOffset=${nextOffset} to continue`);
  }

  return hints;
}
