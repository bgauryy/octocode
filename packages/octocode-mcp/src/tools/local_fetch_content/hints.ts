/**
 * Response-state hints for localGetFileContent.
 * Emits only on empty/error — pagination/cursors are carried in the response
 * envelope, usage guidance lives in the tool description.
 *
 * @module tools/local_fetch_content/hints
 */

import type { HintContext, ToolHintGenerators } from '../../types/metadata.js';

export const hints: ToolHintGenerators = {
  empty: () => [],

  error: (ctx: HintContext = {}) => {
    if (ctx.errorType === 'size_limit' && ctx.isLarge) {
      return ctx.fileSize
        ? [
            `File ~${Math.round(ctx.fileSize / 1024)}KB exceeds the read budget.`,
          ]
        : [];
    }
    return [];
  },
};
