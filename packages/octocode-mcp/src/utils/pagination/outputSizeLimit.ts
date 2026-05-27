/**
 * Output size limit utility for large tool responses.
 *
 * Applies character-based pagination to serialized output when it exceeds
 * the MAX_OUTPUT_CHARS threshold. Used by tools like githubSearchPullRequests
 * and lspCallHierarchy that can produce very large responses.
 *
 * Follows the same pattern as localViewStructure and localFetchContent:
 * - Auto-paginates when output > MAX_OUTPUT_CHARS (2000 chars)
 * - Supports explicit charOffset/charLength for manual pagination
 * - Returns pagination metadata for next-page navigation
 */

import { DEFAULTS, RESOURCE_LIMITS } from '../core/constants.js';
import { getConfigSync } from 'octocode-shared';
import { applyPagination, createPaginationInfo } from './core.js';
import type { PaginationInfo } from '../../types/toolResults.js';

function readConfiguredDefaultCharLength(): number | undefined {
  const config = getConfigSync() as {
    output?: {
      pagination?: {
        defaultCharLength?: number;
      };
    };
  };

  return config.output?.pagination?.defaultCharLength;
}

/**
 * Options for applying output size limits
 */
interface OutputSizeLimitOptions {
  /** Character offset for pagination (0-based) */
  charOffset?: number;
  /** Character length for pagination */
  charLength?: number;
  /** Custom max output chars threshold (defaults to DEFAULTS.MAX_OUTPUT_CHARS) */
  maxOutputChars?: number;
  /** Custom recommended char length for auto-pagination (defaults to RESOURCE_LIMITS.RECOMMENDED_CHAR_LENGTH) */
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
 * When content exceeds MAX_OUTPUT_CHARS and no explicit charLength is provided,
 * auto-paginates with RECOMMENDED_CHAR_LENGTH. When explicit charOffset/charLength
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
  let configuredDefaultCharLength: number | undefined;
  try {
    configuredDefaultCharLength = readConfiguredDefaultCharLength();
  } catch {
    configuredDefaultCharLength = undefined;
  }
  const maxOutputChars =
    options.maxOutputChars ??
    configuredDefaultCharLength ??
    DEFAULTS.MAX_OUTPUT_CHARS;
  const recommendedCharLength =
    options.recommendedCharLength ??
    configuredDefaultCharLength ??
    RESOURCE_LIMITS.RECOMMENDED_CHAR_LENGTH;

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
