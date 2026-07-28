import {
  cleanJsonObject,
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type { BulkFinalizerOutput } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import { relativizeResultPaths, hoistSharedFields } from './pathRelativize.js';

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
  // Dedupe repeated data across result rows BEFORE formatting (so text and
  // structuredContent stay consistent): relativize shared path prefixes into
  // `base`, and hoist scalars identical across every row into `shared`. Mirrors
  // the non-finalize path in bulk/response.ts so finalized tools aren't the odd
  // ones out. Both helpers mutate the rows in place + return the lifted map.
  let effectiveKeys = keysPriority;
  const rows = (responseData as { results?: unknown }).results;
  if (Array.isArray(rows)) {
    const rowRefs = rows as Array<{ data?: unknown }>;
    const base = relativizeResultPaths(rowRefs);
    if (base) (responseData as Record<string, unknown>).base = base;
    const shared = hoistSharedFields(rowRefs);
    if (shared) (responseData as Record<string, unknown>).shared = shared;
    if (base || shared) {
      effectiveKeys = [
        ...(base ? ['base'] : []),
        ...(shared ? ['shared'] : []),
        ...keysPriority,
      ];
    }
  }

  const text = createResponseFormat(
    responseData as Parameters<typeof createResponseFormat>[0],
    [...effectiveKeys]
  );

  return {
    // Clean before sanitizing so structuredContent matches the text channel
    // (createResponseFormat above already runs cleanJsonObject → sanitize).
    structuredContent: sanitizeStructuredContent(
      cleanJsonObject(responseData) ?? {}
    ) as T,
    text,
    isError,
  };
}
