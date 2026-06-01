import { getConfigSync } from 'octocode-shared';
import { DEFAULT_OUTPUT_CONFIG } from 'octocode-shared';

/**
 * THE single pagination char limit for every tool result.
 *
 * One flow, one number: every paginator (auto-pagination trigger AND page size,
 * per-query / bulk / output-size limits, and the LSP content-budget clamps)
 * reads this. The value lives in exactly one place — the resolved config's
 * `output.pagination.defaultCharLength` (default in octocode-shared) — so there
 * are no alias constants to drift. Larger result sets are reached by paginating
 * (charOffset / page / *PerPage), never by returning a bigger single payload.
 */
export function getOutputCharLimit(): number {
  try {
    return getConfigSync().output.pagination.defaultCharLength;
  } catch {
    return DEFAULT_OUTPUT_CONFIG.pagination.defaultCharLength;
  }
}
