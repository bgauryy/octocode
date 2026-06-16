import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { maskSensitiveData } from 'octocode-security/mask';
import { ContentSanitizer } from 'octocode-security/contentSanitizer';
import { getConfigSync } from 'octocode-shared';
import { contextUtils, type JsonInput } from './utils/contextUtils.js';
import type { BulkToolResponse } from './types/bulk.js';
import type { StructuredToolResponse } from './types/toolResults.js';
import type {
  RoleContentBlock,
  RoleBasedResultOptions,
} from './types/responseTypes.js';

function getOutputFormat(): 'yaml' | 'json' {
  try {
    return getConfigSync().output.format;
  } catch {
    return 'yaml';
  }
}
export { StatusEmojis } from './types/responseTypes.js';
export type {
  ContentRole,
  RoleContentBlock,
  RoleBasedResultOptions,
  RoleAnnotations,
} from './types/responseTypes.js';

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

export const ContentBuilder = {
  system(text: string, priority = 1.0): RoleContentBlock {
    return {
      type: 'text',
      text,
      annotations: {
        audience: ['assistant'],
        priority,
        role: 'system',
      },
    };
  },

  assistant(text: string, priority = 0.8): RoleContentBlock {
    return {
      type: 'text',
      text,
      annotations: {
        audience: ['assistant', 'user'],
        priority,
        role: 'assistant',
      },
    };
  },

  user(text: string, priority = 0.6): RoleContentBlock {
    return {
      type: 'text',
      text,
      annotations: {
        audience: ['user'],
        priority,
        role: 'user',
      },
    };
  },

  data(data: unknown, format?: 'yaml' | 'json'): RoleContentBlock {
    const resolvedFormat = format ?? getOutputFormat();
    let text: string;
    try {
      text =
        resolvedFormat === 'yaml'
          ? contextUtils.jsonToYamlString(cleanJsonObject(data) as JsonInput)
          : JSON.stringify(cleanJsonObject(data), null, 2);
    } catch {
      text = 'error: "Data serialization failed"\n';
    }
    return {
      type: 'text',
      text: sanitizeText(text),
      annotations: {
        audience: ['assistant'],
        priority: 0.3,
        role: 'assistant',
      },
    };
  },
};

export const StatusEmoji = {
  success: '✅',
  empty: '📭',
  error: '❌',
  partial: '⚠️',
  searching: '🔍',
  loading: '⏳',
  info: 'ℹ️',
  file: '📄',
  folder: '📁',
  page: '📃',
  definition: '🎯',
  reference: '🔗',
  call: '📞',
} as const;

export function createRoleBasedResult(
  options: RoleBasedResultOptions
): CallToolResult {
  const content: RoleContentBlock[] = [];
  const { system, assistant, user, data, isError } = options;

  if (system) {
    const systemParts: string[] = [];

    if (system.instructions) {
      systemParts.push(system.instructions);
    }

    if (system.pagination) {
      const { currentPage, totalPages, hasMore } = system.pagination;
      systemParts.push(
        `Page ${currentPage}/${totalPages}${hasMore ? ' (more available)' : ''}`
      );
    }

    if (system.warnings?.length) {
      systemParts.push(
        `⚠️ Warnings:\n${system.warnings.map(w => `- ${w}`).join('\n')}`
      );
    }

    if (system.hints?.length) {
      systemParts.push(`Hints:\n${system.hints.map(h => `- ${h}`).join('\n')}`);
    }

    if (systemParts.length > 0) {
      content.push(ContentBuilder.system(systemParts.join('\n\n')));
    }
  }

  content.push(ContentBuilder.assistant(assistant.summary));

  if (assistant.details) {
    const dataFormat =
      assistant.format === 'json' || assistant.format === 'yaml'
        ? assistant.format
        : undefined;
    content.push(ContentBuilder.data(assistant.details, dataFormat));
  }

  if (user) {
    const userMessage = user.emoji
      ? `${user.emoji} ${user.message}`
      : user.message;
    content.push(ContentBuilder.user(userMessage));
  }

  return {
    content,
    structuredContent: cleanAndStructure(data),
    isError: Boolean(isError),
  };
}

export const QuickResult = {
  success(summary: string, data: unknown, hints?: string[]): CallToolResult {
    return createRoleBasedResult({
      system: hints ? { hints } : undefined,
      assistant: { summary },
      user: { message: 'Operation completed', emoji: StatusEmoji.success },
      data,
    });
  },

  empty(message: string, hints?: string[]): CallToolResult {
    return createRoleBasedResult({
      system: {
        hints: hints || ['Try broader search terms', 'Check spelling'],
      },
      assistant: { summary: message },
      user: { message: 'No results found', emoji: StatusEmoji.empty },
      data: { status: 'empty', results: [] },
    });
  },

  error(error: string, details?: unknown): CallToolResult {
    return createRoleBasedResult({
      system: {
        instructions:
          'Tool execution failed. Error details provided for self-correction.',
      },
      assistant: { summary: `Error: ${error}` },
      user: { message: 'An error occurred', emoji: StatusEmoji.error },
      data: { status: 'error', error, details },
      isError: true,
    });
  },

  paginated(
    summary: string,
    data: unknown,
    pagination: { page: number; total: number; hasMore: boolean },
    hints?: string[]
  ): CallToolResult {
    return createRoleBasedResult({
      system: {
        pagination: {
          currentPage: pagination.page,
          totalPages: pagination.total,
          hasMore: pagination.hasMore,
        },
        hints,
      },
      assistant: { summary },
      user: {
        message: `Page ${pagination.page} of ${pagination.total}`,
        emoji: pagination.hasMore ? '📄' : StatusEmoji.success,
      },
      data,
    });
  },
};

function cleanAndStructure(data: unknown): Record<string, unknown> | undefined {
  if (data === null || data === undefined) {
    return undefined;
  }
  const cleaned = cleanJsonObject(data);
  if (
    typeof cleaned === 'object' &&
    cleaned !== null &&
    !Array.isArray(cleaned)
  ) {
    return sanitizeStructuredContent(
      cleaned as Record<string, unknown>
    ) as Record<string, unknown>;
  }
  const wrapped = { data: cleaned };
  return sanitizeStructuredContent(wrapped) as Record<string, unknown>;
}

function sanitizeText(text: string): string {
  if (text == null || typeof text !== 'string') return '';
  const sanitizationResult = ContentSanitizer.sanitizeContent(text);
  return maskSensitiveData(sanitizationResult.content);
}

export function sanitizeStructuredContent(obj: unknown): unknown {
  if (obj === null || obj === undefined) return obj;

  if (typeof obj === 'string') {
    const sanitized = ContentSanitizer.sanitizeContent(obj);
    return maskSensitiveData(sanitized.content);
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
  const outputFormat = getOutputFormat();
  const defaultPriority =
    'results' in cleanedData
      ? ['results', 'id', 'status', 'data']
      : ['instructions', 'status', 'data'];

  let serialized: string;
  if (outputFormat === 'json') {
    const priority = keysPriority || defaultPriority;
    serialized = JSON.stringify(sortObjectKeys(cleanedData, priority), null, 2);
  } else {
    serialized = contextUtils.jsonToYamlString(cleanedData as JsonInput, {
      keysPriority: keysPriority || defaultPriority,
    });
  }

  const sanitizationResult = ContentSanitizer.sanitizeContent(serialized);
  return maskSensitiveData(sanitizationResult.content);
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

function isTrivialPagination(value: unknown): boolean {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return false;
  }
  const p = value as Record<string, unknown>;
  const keys = Object.keys(p);
  if (keys.length === 0 || !keys.every(k => PAGINATION_KEYS.has(k))) {
    return false;
  }
  if (p.hasMore !== false) return false;
  if (typeof p.totalPages === 'number') return p.totalPages <= 1;
  if ('charOffset' in p || 'nextCharOffset' in p) {
    return (p.charOffset ?? 0) === 0;
  }
  return true;
}

function cleanJsonObject(
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
