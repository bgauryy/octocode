import { z } from 'zod';
import { FileContentQuerySchema as CoreFileContentQuerySchema } from '../../toolContract/input/resources/tools/ghGetFileContent.js';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  offsetField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { createContentSelectorQuerySchema } from '../../scheme/conditionalSchemas.js';
import type {
  CharPagination,
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type {
  BulkToolResultRow,
  ResponsePaginationInfo,
} from '../../types/toolOutput.js';

// No schema-level default: the direct-tool executor parses inputSchema (applying
// any default) before execution runs, which would erase the distinction between
// "caller omitted minify" and "caller chose standard". The effective default is
// resolved in execution instead — 'none' for fullContent, 'standard' otherwise.
const minifyField = z.enum(['none', 'standard', 'symbols']).optional();

const queryOverrides = {
  startLine: lineNumberField,
  endLine: lineNumberField,
  contextLines: contextLinesField,
  charOffset: offsetField.optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
  minify: minifyField,
} as const;

export const FileContentQueryBaseLocalSchema = createQueryShapeSchema(
  CoreFileContentQuerySchema,
  queryOverrides
);

export const FileContentQueryLocalSchema = describeQuerySchema(
  CoreFileContentQuerySchema,
  queryOverrides
);

export const FileContentBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createContentSelectorQuerySchema(FileContentQueryBaseLocalSchema, {
    githubDirectoryMode: true,
  })
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what ghGetFileContent returns. No zod: the MCP
// server registers no outputSchema. The result rows carry an OPTIONAL `data`
// (error rows omit it), so this uses a bespoke envelope rather than the
// data-required BulkToolOutput generic.
// ---------------------------------------------------------------------------

// Parity with local_fetch_content: compose shared char/item pagination.
export type GitHubFetchFilePagination =
  | (CharPagination & { nextPage?: number })
  | (ItemPagination & {
      charOffset?: number;
      charLength?: number;
      totalChars?: number;
      nextCharOffset?: number;
    });

export interface GitHubFetchFileEntry {
  path: string;
  content: string;
  localPath?: string;
  repoRoot?: string;
  // isSkeleton was dropped — always equal to contentView==='symbols', so it
  // carried no information a consumer couldn't already derive from contentView.
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  resolvedBranch?: string;
  commitSha?: string;
  pagination?: GitHubFetchFilePagination;
  next?: Record<string, ToolContinuation>;
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
  cached?: boolean;
}

export interface GitHubFetchDirectoryEntry {
  path: string;
  localPath: string;
  repoRoot?: string;
  fileCount: number;
  totalSize: number;
  complete?: boolean;
  verified?: boolean;
  commitSha?: string;
  hasSubdirectories?: boolean;
  skippedSummary?: Record<string, number>;
  directoryEntryCount?: number;
  eligibleFileCount?: number;
  savedFileCount?: number;
  skipped?: {
    nonFile: number;
    oversized: number;
    binary: number;
    fileLimit: number;
    fetchFailed: number;
    totalSizeLimit: number;
    pathTraversal: number;
  };
  limits?: {
    maxDirectoryFiles: number;
    maxTotalSize: number;
    maxFileSize: number;
  };
  warnings?: string[];
  files?: Array<{ path: string; size: number; type: string }>;
  cached?: boolean;
  resolvedBranch?: string;
  isPartial?: boolean;
  terminalLimit?: boolean;
  partialReasons?: string[];
  next?: Record<string, ToolContinuation>;
}

export interface GitHubFetchContentData {
  owner: string;
  repo: string;
  files?: GitHubFetchFileEntry[];
  directories?: GitHubFetchDirectoryEntry[];
}

export interface GitHubFetchContentErrorData {
  owner?: string;
  repo?: string;
  path?: string;
  error: string;
}

export interface GitHubFetchContentOutputLocal {
  base?: string;
  shared?: Record<string, string | number | boolean>;
  responsePagination?: ResponsePaginationInfo;
  results: Array<
    BulkToolResultRow<GitHubFetchContentData | GitHubFetchContentErrorData>
  >;
  // Index signature: satisfies BulkFinalizer's `TOutput extends
  // Record<string, unknown>` constraint (the old zod-inferred type did too).
  [key: string]: unknown;
}
