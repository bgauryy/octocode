import type { ToolCallResult } from '../../types.js';

export function planResult(text: string, details?: unknown): ToolCallResult {
  return { content: [{ type: 'text', text }], ...(details === undefined ? {} : { details }) };
}

export function planError(text: string, details?: unknown): ToolCallResult {
  return { content: [{ type: 'text', text }], isError: true, ...(details === undefined ? {} : { details }) };
}

export function withPlanPersistenceWarning(result: ToolCallResult, warning: string | undefined): ToolCallResult {
  if (!warning) return result;
  const warningText = `[PLAN] warning: the host could not persist this change (${warning}). It remains memory-only and will not survive session recovery.`;
  const details = result.details && typeof result.details === 'object' && !Array.isArray(result.details)
    ? { ...result.details, persistenceWarning: warning }
    : { persistenceWarning: warning };
  return { ...result, content: [...result.content, { type: 'text', text: warningText }], details };
}
