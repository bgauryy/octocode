import {
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type { BulkFinalizerOutput } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';

export type QueryWithPagination = {
  id?: unknown;
  charLength?: unknown;
  charOffset?: unknown;
};

function unwrapProviderError(value: unknown): {
  message: string;
  status?: number;
} {
  if (typeof value === 'string') return { message: value };
  if (typeof value === 'object' && value !== null) {
    const obj = value as { error?: unknown; status?: unknown };
    const message =
      typeof obj.error === 'string' && obj.error.length > 0
        ? obj.error
        : 'Provider error';
    const status =
      typeof obj.status === 'number' && Number.isFinite(obj.status)
        ? obj.status
        : undefined;
    return { message, status };
  }
  return { message: 'Provider error' };
}

export function collectFlatErrors(
  results: readonly FlatQueryResult[]
): Array<{ id: string; error: string }> {
  const errors: Array<{ id: string; error: string }> = [];
  for (const result of results) {
    if (result.status !== 'error') continue;
    const { message, status } = unwrapProviderError(
      (result.data as { error?: unknown }).error
    );
    const errorMessage =
      status !== undefined ? `${message} (HTTP ${status})` : message;
    errors.push({
      id: result.id,
      error: errorMessage,
    });
  }
  return errors;
}

export function formatFinalizedResponse<T extends Record<string, unknown>>(
  responseData: T,
  keysPriority: readonly string[],
  isError?: boolean
): BulkFinalizerOutput<T> {
  const text = createResponseFormat(
    responseData as Parameters<typeof createResponseFormat>[0],
    [...keysPriority]
  );

  return {
    structuredContent: sanitizeStructuredContent(responseData) as T,
    text,
    isError,
  };
}
