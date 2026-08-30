import {
  cleanJsonObject,
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import { hoistSharedFields, relativizeResultPaths } from './pathRelativize.js';

export function buildResponseChannels<T extends object>(
  responseData: T,
  keysPriority: readonly string[]
): { text: string; structuredContent: T } {
  const responseRecord = responseData as Record<string, unknown>;
  let effectiveKeys = keysPriority;
  const rows = (responseData as { results?: unknown }).results;

  if (Array.isArray(rows)) {
    const rowRefs = rows as Array<{ data?: unknown }>;
    const base = relativizeResultPaths(rowRefs);
    if (base) responseRecord.base = base;
    const shared = hoistSharedFields(rowRefs);
    if (shared) responseRecord.shared = shared;
    if (base || shared) {
      effectiveKeys = [
        ...(base ? ['base'] : []),
        ...(shared ? ['shared'] : []),
        ...keysPriority,
      ];
    }
  }

  return {
    text: createResponseFormat(
      responseRecord as Parameters<typeof createResponseFormat>[0],
      [...effectiveKeys]
    ),
    structuredContent: sanitizeStructuredContent(
      cleanJsonObject(responseRecord) ?? {}
    ) as T,
  };
}
