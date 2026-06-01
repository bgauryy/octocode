/**
 * Output size limit utility for large tool responses.
 *
 * Applies character-based pagination to serialized output when it exceeds the
 * single pagination limit (getOutputCharLimit()). Used by tools like
 * githubSearchPullRequests and lspCallHierarchy that can produce large output.
 *
 * - Auto-paginates when output > getOutputCharLimit() (the one limit)
 * - Supports explicit charOffset/charLength for manual pagination
 * - Returns pagination metadata for next-page navigation
 */

import { applyPagination, createPaginationInfo } from './core.js';
import { getOutputCharLimit } from './charLimit.js';
import type { PaginationInfo } from '../../types/toolResults.js';

/**
 * Options for applying output size limits
 */
interface OutputSizeLimitOptions {
  /** Character offset for pagination (0-based) */
  charOffset?: number;
  /** Character length for pagination */
  charLength?: number;
  /** Override the trigger threshold (defaults to getOutputCharLimit()). */
  maxOutputChars?: number;
  /** Override the auto-pagination page size (defaults to getOutputCharLimit()). */
  recommendedCharLength?: number;
}

/**
 * Result of applying output size limits
 */
interface OutputSizeLimitResult {
  /** The (possibly paginated) content */
  content: string;
  /** Whether the content was limited/paginated */
  wasLimited: boolean;
  /** Pagination metadata (present when content was paginated) */
  pagination?: PaginationInfo;
  /** Warnings about auto-pagination */
  warnings: string[];
  /** Pagination navigation hints for the consumer (always an array) */
  paginationHints: string[];
}

/**
 * Apply output size limits to serialized content.
 *
 * When content exceeds the limit and no explicit charLength is provided,
 * auto-paginates at getOutputCharLimit(). When explicit charOffset/charLength
 * are provided, applies exact pagination.
 *
 * @param content - Serialized content string to check/paginate
 * @param options - Pagination options
 * @returns Result with possibly paginated content and metadata
 */
export function applyOutputSizeLimit(
  content: string,
  options: OutputSizeLimitOptions
): OutputSizeLimitResult {
  // One limit, one place: trigger AND page size both come from getOutputCharLimit().
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

/**
 * Generate navigation hints for paginated output
 */
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
