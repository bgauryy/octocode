import { getDynamicHints, hasDynamicHints } from './dynamic.js';
import type { HintContext, HintStatus } from './types.js';

export function getHints(
  toolName: string,
  status: HintStatus,
  context?: HintContext
): string[] {
  if (!hasDynamicHints(toolName)) return [];
  const hints = getDynamicHints(toolName, status, context);
  return [...new Set(hints)];
}
