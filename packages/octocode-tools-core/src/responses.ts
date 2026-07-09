import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { ContentSanitizer } from '@octocodeai/octocode-engine/contentSanitizer';
import { getConfigSync } from '@octocodeai/config';
import { contextUtils, type JsonInput } from './utils/contextUtils.js';
import type { BulkToolResponse } from './types/bulk.js';
import type { StructuredToolResponse } from './types/toolResults.js';

function getOutputFormat(): 'yaml' | 'json' {
  try {
    return getConfigSync().output.format;
  } catch {
    return 'yaml';
  }
}

export type CallToolResultOutputMode = 'text' | 'json';

export function createResult(options: {
  data: unknown;
  instructions?: string;
  isError?: boolean;
}): CallToolResult {
  if (options == null || typeof options !== 'object') {
    return {
      content: [{ type: 'text', text: 'error: "Invalid result options"\n' }],
      isError: true,
    };
  }
  const { data, instructions, isError } = options;
  const response: StructuredToolResponse = {
    data,
    instructions,
  };

  return {
    content: [{ type: 'text', text: createResponseFormat(response) }],
    isError: Boolean(isError),
  };
}

export function sanitizeStructuredContent(obj: unknown): unknown {
  if (obj === null || obj === undefined) return obj;

  if (typeof obj === 'string') {
    return ContentSanitizer.sanitizeContent(obj).content;
  }

  if (Array.isArray(obj)) {
    return obj.map(item => sanitizeStructuredContent(item));
  }

  if (typeof obj === 'object') {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
      result[key] = sanitizeStructuredContent(value);
    }
    return result;
  }

  return obj;
}

export function formatCallToolResultForOutput(
  result: Pick<CallToolResult, 'content' | 'structuredContent' | 'isError'>,
  outputMode: CallToolResultOutputMode
): string {
  if (outputMode === 'json') {
    return JSON.stringify(result);
  }

  const textBlocks = Array.isArray(result.content)
    ? result.content
        .map(block =>
          'text' in block && typeof block.text === 'string' ? block.text : ''
        )
        .filter(block => block.length > 0)
    : [];

  if (textBlocks.length > 0) {
    return textBlocks.join('\n\n');
  }

  if (result.structuredContent !== undefined) {
    return JSON.stringify(result.structuredContent, null, 2);
  }

  return JSON.stringify(result, null, 2);
}

export function createResponseFormat(
  responseData: StructuredToolResponse | BulkToolResponse,
  keysPriority?: string[]
): string {
  const cleanedData = (cleanJsonObject(responseData) ?? {}) as
    | StructuredToolResponse
    | BulkToolResponse;

  // Sanitize PER FIELD before serializing — not on the final serialized blob.
  // The secret scanner is superlinear on large code blobs (an 8 MB result took
  // ~20 s as one scan, and a 312 KB code blob ~780 ms), while sanitizing the
  // same content field-by-field is linear and ~100x faster (tens of ms). Every
  // string value is still scanned, so redaction is unchanged; this only removes
  // the pathological single-pass scan over the whole document. Mirrors how
  // structuredContent is sanitized (sanitizeStructuredContent).
  const sanitizedData = sanitizeStructuredContent(cleanedData) as
    | StructuredToolResponse
    | BulkToolResponse;

  const outputFormat = getOutputFormat();
  const defaultPriority =
    'results' in sanitizedData
      ? ['results', 'id', 'status', 'data']
      : ['instructions', 'status', 'data'];

  if (outputFormat === 'json') {
    const priority = keysPriority || defaultPriority;
    return JSON.stringify(sortObjectKeys(sanitizedData, priority), null, 2);
  }
  return contextUtils.jsonToYamlString(sanitizedData as JsonInput, {
    keysPriority: keysPriority || defaultPriority,
  });
}

function sortObjectKeys(obj: unknown, priority: string[]): unknown {
  if (obj === null || obj === undefined) return obj;
  if (Array.isArray(obj))
    return obj.map(item => sortObjectKeys(item, priority));
  if (typeof obj !== 'object') return obj;

  const record = obj as Record<string, unknown>;
  const sorted: Record<string, unknown> = {};

  for (const key of priority) {
    if (key in record) sorted[key] = sortObjectKeys(record[key], priority);
  }
  for (const key of Object.keys(record)) {
    if (!(key in sorted)) sorted[key] = sortObjectKeys(record[key], priority);
  }

  return sorted;
}

const PAGINATION_KEYS = new Set([
  'currentPage',
  'totalPages',
  'perPage',
  'itemsPerPage',
  'entriesPerPage',
  'filesPerPage',
  'totalMatches',
  'totalFiles',
  'totalEntries',
  'totalItems',
  'totalResults',
  'totalReferences',
  'reportedTotalMatches',
  'reachableTotalMatches',
  'totalMatchesKind',
  'totalMatchesCapped',
  'hasMore',
  'nextPage',
  'charOffset',
  'charLength',
  'totalChars',
  'totalBytes',
  'nextCharOffset',
]);

// Total-count fields: when any is present and positive, the pagination object
// carries "all N matches/items" information that the text channel would lose if
// stripped — so such pagination is NOT trivial even with hasMore=false.
const TOTAL_COUNT_KEYS = [
  'totalMatches',
  'totalItems',
  'totalFiles',
  'totalEntries',
  'totalResults',
  'totalReferences',
  'reportedTotalMatches',
  'reachableTotalMatches',
] as const;

function isTrivialPagination(value: unknown): boolean {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false;
  }
  const p = value as Record<string, unknown>;
  const keys = Object.keys(p);
  if (keys.length === 0 || !keys.every(k => PAGINATION_KEYS.has(k))) {
    return false;
  }
  if (
    TOTAL_COUNT_KEYS.some(k => typeof p[k] === 'number' && (p[k] as number) > 0)
  ) {
    return false;
  }
  if (p.hasMore !== false) return false;
  if (typeof p.totalPages === 'number') return p.totalPages <= 1;
  if ('charOffset' in p || 'nextCharOffset' in p) {
    return (p.charOffset ?? 0) === 0;
  }
  return true;
}

export function cleanJsonObject(
  obj: unknown,
  context: { inFilesObject?: boolean; depth?: number } = {}
): unknown {
  if (obj === null || obj === undefined || Number.isNaN(obj)) {
    return undefined;
  }

  const { inFilesObject = false, depth = 0 } = context;

  if (Array.isArray(obj)) {
    const cleaned = obj
      .map(item => cleanJsonObject(item, { inFilesObject, depth: depth + 1 }))
      .filter(item => item !== undefined);
    const isCodeSearchPathMatch = inFilesObject && depth >= 2;
    return cleaned.length > 0 || isCodeSearchPathMatch ? cleaned : undefined;
  }

  if (typeof obj === 'object' && obj !== null) {
    const cleaned: Record<string, unknown> = {};
    let hasValidProperties = false;

    for (const [key, value] of Object.entries(obj)) {
      if (
        key === 'results' &&
        depth === 0 &&
        Array.isArray(value) &&
        value.length === 0
      ) {
        cleaned[key] = [];
        hasValidProperties = true;
        continue;
      }

      if (isTrivialPagination(value)) {
        continue;
      }

      const enteringFilesObject =
        (key === 'files' || key === 'repositories') && !inFilesObject;
      const cleanedValue = cleanJsonObject(value, {
        inFilesObject: inFilesObject || enteringFilesObject,
        depth: enteringFilesObject ? 0 : depth + 1,
      });
      if (cleanedValue !== undefined) {
        cleaned[key] = cleanedValue;
        hasValidProperties = true;
      }
    }

    return hasValidProperties ? cleaned : undefined;
  }

  return obj;
}
