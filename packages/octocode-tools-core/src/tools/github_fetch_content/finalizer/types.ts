import type { z } from 'zod';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schema';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type {
  FetchPagination,
  LocalFetchToolResult,
} from '@octocodeai/octocode-core/extra-types';
import type { ToolContinuation } from '../../../scheme/pagination.js';
import type { QueryWithPagination } from '../../../utils/response/groupedFinalizer.js';
import type { WithOptionalMeta } from '../../../types/execution.js';

export type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

export type FileEntry = {
  path: string;
  content: string;
  sourceLineRanges?: Array<{ start: number; end: number }>;
  /** Coarse file bucket to guide how bytes are read; omitted when uncertain. */
  fileType?: 'code' | 'config' | 'lock' | 'doc';
  contentView?: 'none' | 'standard' | 'symbols';
  totalLines?: number;
  sourceChars?: number;
  sourceBytes?: number;
  returnedChars?: number;
  returnedBytes?: number;
  returnedLines?: number;
  selectedMatchCount?: number;
  minifyFallback?: LocalFetchToolResult['minifyFallback'];
  resolvedBranch?: string;
  commitSha?: string;
  pagination?: FetchPagination;
  isPartial?: boolean;
  errorCode?: 'contentSecurityLimit' | 'fullContentLimit' | 'noMatches';
  terminalLimit?: boolean;
  partialReasons?: Array<
    'security-selected-view-size-limit' | 'full-content-size-limit'
  >;
  startLine?: number;
  endLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  /** Exact matched-line numbers; matchRanges span the selected source windows. */
  matchedLines?: number[];
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
  matchNotFound?: boolean;
  searchedFor?: string;
  cached?: boolean;
  next?: FileContentNextMap;
};

export type FileContentNextMap = Partial<
  Record<'continue' | 'readBoundedLines', ToolContinuation>
>;
