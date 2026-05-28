/**
 * Unified hints system for all tools
 *
 * Emits only dynamic, context-aware hints. Static guidance lives in tool
 * descriptions / SKILL.md, never in tool responses.
 *
 * @module hints
 */

import { getDynamicHints, hasDynamicHints } from './dynamic.js';
import type { HintContext, HintStatus } from './types.js';

/**
 * Get hints for a tool response.
 *
 * Returns conditional, response-state-derived hints only:
 * pagination cursors, error-specific guidance, empty-result pointers.
 * Generic "how to use this tool" text is intentionally excluded.
 */
export function getHints(
  toolName: string,
  status: HintStatus,
  context?: HintContext
): string[] {
  // Hints only fire on empty/error (enforced by HintStatus). On success the
  // response data + pagination/evidence/warnings carry the signal — usage
  // guidance lives in the tool description.
  if (!hasDynamicHints(toolName)) return [];
  const hints = getDynamicHints(toolName, status, context);
  return [...new Set(hints)];
}
