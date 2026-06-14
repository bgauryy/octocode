export type ContentRole = 'system' | 'assistant' | 'user';

export interface RoleAnnotations {
  audience?: Array<'user' | 'assistant'>;

  priority?: number;

  role?: ContentRole;

  lastModified?: string;
}

export interface RoleContentBlock {
  type: 'text';
  text: string;
  annotations?: RoleAnnotations;
}

interface ResponsePagination {
  currentPage: number;
  totalPages: number;
  hasMore: boolean;

  perPage?: number;

  totalItems?: number;
}

interface SystemContentOptions {
  instructions?: string;

  hints?: string[];

  pagination?: ResponsePagination;

  warnings?: string[];
}

interface AssistantContentOptions {
  summary: string;

  details?: string;

  format?: 'yaml' | 'json' | 'markdown' | 'plain';
}

interface UserContentOptions {
  message: string;

  emoji?: string;
}

export interface RoleBasedResultOptions {
  system?: SystemContentOptions;

  assistant: AssistantContentOptions;

  user?: UserContentOptions;

  data: unknown;

  isError?: boolean;
}

export const StatusEmojis = {
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
