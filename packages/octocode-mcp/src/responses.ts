import { CallToolResult } from '@modelcontextprotocol/sdk/types';
import { maskSensitiveData } from 'octocode-security-utils/mask';
import { ContentSanitizer } from 'octocode-security-utils/contentSanitizer';
import { jsonToYamlString } from './utils/minifier/jsonToYamlString.js';
import { getConfigSync } from 'octocode-shared';
import type { BulkToolResponse, StructuredToolResponse } from './types.js';
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

// Response patterns:
// - **Bulk tools** (githubSearchCode, localSearchCode, etc.): Use createResponseFormat
//   via bulk.ts → createBulkResponse. Single YAML block with instructions + results[].
// - **Role-based** (createRoleBasedResult, ContentBuilder, QuickResult): For single-result
//   or non-bulk tools that need structured role separation (system/assistant/user).
//   Currently exported for future use; bulk tools do not use this pattern.

/**
 * Content block builder for role-based responses.
 * Creates content blocks with appropriate annotations for each role type.
 *
 * Roles:
 * - system: Instructions, hints, pagination (agent-only, high priority)
 * - assistant: Formatted data, summaries (shown to both agent and user)
 * - user: Human-friendly messages (primarily for user display)
 */
export const ContentBuilder = {
  /**
   * System content: Instructions for the agent (hidden from user)
   * High priority (1.0) - processed first by the agent
   */
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

  /**
   * Assistant content: Formatted response for agent reasoning
   * Medium-high priority (0.8) - main content for the agent
   */
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

  /**
   * User content: Human-friendly summary
   * Medium priority (0.6) - shown to user in UI
   */
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

  /**
   * Data content: Serialized data block
   * Low priority (0.3) - detailed data for agent reference
   */
  data(data: unknown, format?: 'yaml' | 'json'): RoleContentBlock {
    const resolvedFormat = format ?? getOutputFormat();
    let text: string;
    try {
      text =
        resolvedFormat === 'yaml'
          ? jsonToYamlString(cleanJsonObject(data))
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

/**
 * Status emoji constants for consistent visual feedback
 */
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

/**
 * Create a role-based tool result with proper content separation.
 *
 * This produces MCP-compliant responses with:
 * - Multiple content blocks with role annotations
 * - structuredContent for machine-readable data
 * - Proper isError flag for error handling
 *
 * @example
 * ```typescript
 * createRoleBasedResult({
 *   system: { hints: ['Use lineHint for LSP tools'] },
 *   assistant: { summary: 'Found 3 files matching pattern' },
 *   user: { message: 'Search complete', emoji: '✅' },
 *   data: { files: [...], totalMatches: 3 }
 * });
 * ```
 */
export function createRoleBasedResult(
  options: RoleBasedResultOptions
): CallToolResult {
  const content: RoleContentBlock[] = [];
  const { system, assistant, user, data, isError } = options;

  // 1. System block (highest priority) - instructions for agent
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

  // 2. Assistant block (formatted data for agent reasoning)
  content.push(ContentBuilder.assistant(assistant.summary));

  if (assistant.details) {
    const dataFormat =
      assistant.format === 'json' || assistant.format === 'yaml'
        ? assistant.format
        : undefined;
    content.push(ContentBuilder.data(assistant.details, dataFormat));
  }

  // 3. User block (human-friendly summary)
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

/**
 * Quick result helpers for common response patterns
 */
export const QuickResult = {
  /**
   * Success result with data and optional hints
   */
  success(summary: string, data: unknown, hints?: string[]): CallToolResult {
    return createRoleBasedResult({
      system: hints ? { hints } : undefined,
      assistant: { summary },
      user: { message: 'Operation completed', emoji: StatusEmoji.success },
      data,
    });
  },

  /**
   * Empty result with suggestions
   */
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

  /**
   * Error result with details for self-correction
   */
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

  /**
   * Paginated result with navigation info
   */
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

/**
 * Clean data and prepare for structuredContent
 */
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
  // Wrap non-object data
  const wrapped = { data: cleaned };
  return sanitizeStructuredContent(wrapped) as Record<string, unknown>;
}

/**
 * Sanitize text content (mask secrets, sanitize content)
 */
function sanitizeText(text: string): string {
  if (text == null || typeof text !== 'string') return '';
  const sanitizationResult = ContentSanitizer.sanitizeContent(text);
  return maskSensitiveData(sanitizationResult.content);
}

/**
 * Deep-walk an object and sanitize all string values.
 * Applied to structuredContent so secrets never leak via the machine-readable channel.
 */
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
    serialized = jsonToYamlString(cleanedData, {
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
    // Preserve empty arrays for code search path results (files > repo > path level)
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
