import { contextUtils } from '../../../utils/contextUtils.js';
import { trimDiffContext } from '../../../utils/parsers/diff.js';
import type { QueryLike } from './pagination.js';

// Keep providers/cache entries exact. Transform a complete selected view once,
// before calculating offsets; matching reads preserve their source anchors.
export function historyBodyView(body: string, query: QueryLike): string {
  if (query.minify !== 'standard' || query.matchString) return body;
  return contextUtils.minifyMarkdownCore(body);
}

export function historyPatchView(patch: string, query: QueryLike): string {
  if (query.minify !== 'standard' || query.matchString) return patch;
  return trimDiffContext(patch);
}
