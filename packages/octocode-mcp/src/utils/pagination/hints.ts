/**
 * Pagination hint generation utilities.
 *
 * Strict policy: emit a hint only when it is either
 *  (a) a pagination cursor the agent can re-call with, or
 *  (b) a recovery directive for a size/over-budget condition.
 *
 * No token narration, no "Complete page" tautologies, no emoji decoration,
 * no echo of the params the caller already has.
 */

import type { PaginationInfo } from '../../types/toolResults.js';
import type {
  PaginationMetadata,
  GeneratePaginationHintsOptions,
  GitHubFileContentHintContext,
  StructurePaginationInfo,
  StructurePaginationHintContext,
} from './types.js';

/**
 * Surface token-budget recovery directives only when the response is
 * actually at risk. Below 30K tokens we say nothing — the agent already
 * sees the data and doesn't need a "you're fine" reassurance.
 */
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

/**
 * Generic pagination navigation. Only emits a cursor when more pages
 * exist. Final-page tautologies are silent.
 */
function generateNavigationHints(metadata: PaginationMetadata): string[] {
  if (metadata.hasMore && metadata.nextCharOffset !== undefined) {
    return [
      `Page ${metadata.currentPage}/${metadata.totalPages}. Next: charOffset=${metadata.nextCharOffset}`,
    ];
  }
  return [];
}

/**
 * Pagination hints based on metadata (generic, for local tools).
 */
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

/**
 * GitHub file-content pagination. Only fires when more pages exist; emits
 * a single cursor line the agent can use directly.
 */
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

/**
 * Repository structure pagination. Only fires when more pages exist;
 * emits a single cursor line, no param echo or "tip" recipes.
 */
export function generateStructurePaginationHints(
  pagination: StructurePaginationInfo,
  _context: StructurePaginationHintContext
): string[] {
  if (!pagination.hasMore) return [];

  return [
    `Page ${pagination.currentPage}/${pagination.totalPages}. Next: entryPageNumber=${pagination.currentPage + 1}`,
  ];
}
