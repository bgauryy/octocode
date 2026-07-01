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

export type FlatErrorEntry = {
  id: string;
  error: string;
  status?: number;
  retryAfterSeconds?: number;
  rateLimitRemaining?: number;
  rateLimitReset?: number;
};

function finiteNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

function unwrapProviderError(value: unknown): {
  message: string;
  status?: number;
  retryAfterSeconds?: number;
  rateLimitRemaining?: number;
  rateLimitReset?: number;
} {
  if (typeof value === 'string') return { message: value };
  if (typeof value === 'object' && value !== null) {
    const obj = value as {
      error?: unknown;
      status?: unknown;
      retryAfter?: unknown;
      rateLimitRemaining?: unknown;
      rateLimitReset?: unknown;
    };
    const message =
      typeof obj.error === 'string' && obj.error.length > 0
        ? obj.error
        : 'Provider error';
    return {
      message,
      status: finiteNumber(obj.status),
      retryAfterSeconds: finiteNumber(obj.retryAfter),
      rateLimitRemaining: finiteNumber(obj.rateLimitRemaining),
      rateLimitReset: finiteNumber(obj.rateLimitReset),
    };
  }
  return { message: 'Provider error' };
}

export function collectFlatErrors(
  results: readonly FlatQueryResult[]
): FlatErrorEntry[] {
  const errors: FlatErrorEntry[] = [];
  for (const result of results) {
    if (result.status !== 'error') continue;
    const {
      message,
      status,
      retryAfterSeconds,
      rateLimitRemaining,
      rateLimitReset,
    } = unwrapProviderError((result.data as { error?: unknown }).error);
    const errorMessage =
      status !== undefined ? `${message} (HTTP ${status})` : message;
    errors.push({
      id: result.id,
      error: errorMessage,
      ...(status !== undefined ? { status } : {}),
      ...(retryAfterSeconds !== undefined ? { retryAfterSeconds } : {}),
      ...(rateLimitRemaining !== undefined ? { rateLimitRemaining } : {}),
      ...(rateLimitReset !== undefined ? { rateLimitReset } : {}),
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
