import { z } from 'zod';
import { FetchContentQuerySchema as CoreFetchContentQuerySchema } from '@octocodeai/octocode-core/schemas';
import { MAX_CHAR_LENGTH } from '../../config.js';
import {
  clampedInt,
  contextLinesField,
  createRelaxedBulkQuerySchema,
  lineNumberField,
  type MinifyMode,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  CharPagination,
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

// No schema-level default: the direct-tool executor parses inputSchema (applying
// any default) before execution runs, which would erase the distinction between
// "caller omitted minify" and "caller chose standard". The effective default is
// resolved in fetchContent instead — 'none' for fullContent (verbatim, or
// "returns the whole file" would silently strip comments), 'standard' otherwise.
const minifyField = z.enum(['none', 'standard', 'symbols']).optional();

const queryOverrides = {
  startLine: lineNumberField,
  endLine: lineNumberField,
  contextLines: contextLinesField.default(5),
  charOffset: clampedInt(0, 100_000_000).optional(),
  charLength: clampedInt(1, MAX_CHAR_LENGTH).optional(),
  minify: minifyField,
} as const;

const FetchContentQueryShape = createQueryShapeSchema(
  CoreFetchContentQuerySchema,
  queryOverrides
);

export const LocalFetchContentQuerySchema = describeQuerySchema(
  CoreFetchContentQuerySchema,
  queryOverrides
);

export type FetchContentQuery = z.infer<typeof LocalFetchContentQuerySchema> & {
  minify?: MinifyMode;
};

export const LocalFetchContentBulkQuerySchema = createRelaxedBulkQuerySchema(
  FetchContentQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output schema — describes what localGetFileContent returns per query result.
//
// A single query can return either:
//   - a char-paginated content window (startLine/endLine / matchString / full)
//   - a line-range extraction result
// Both modes share the same result row shape; pagination discriminates.
// ---------------------------------------------------------------------------

export interface FileContentMatchRange {
  start: number;
  end: number;
}

export interface LocalGetFileContentData {
  path?: string;
  absolutePath?: string;
  uri?: string;
  content?: string;
  // isSkeleton was dropped — always equal to contentView==='symbols', so it
  // carried no information a consumer couldn't already derive from contentView.
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  // Chars actually returned in `content` after minification + windowing —
  // compare against sourceChars to see what a contentView saved (mirrors
  // ghGetFileContent's per-view fileSize signal).
  returnedChars?: number;
  startLine?: number;
  endLine?: number;
  isPartial?: boolean;
  matchRanges?: FileContentMatchRange[];
  // Char pagination for content windows
  pagination?: (CharPagination & { nextBlockChar?: number }) | ItemPagination;
  next?: Record<string, ToolContinuation>;
  modified?: string;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
}

export type LocalGetFileContentOutput = BulkToolOutput<LocalGetFileContentData>;
