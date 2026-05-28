/**
 * Dynamic hints for packageSearch tool
 * @module tools/package_search/hints
 */

import type { ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  // Package-specific empty/error hints (no-match variations, deprecation) are
  // injected inline in execution.ts via extraHints.
  empty: () => [],
  error: () => [],
};
