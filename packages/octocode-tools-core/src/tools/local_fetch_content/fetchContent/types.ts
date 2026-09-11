import type { LocalFetchToolResult } from '@octocodeai/octocode-core/extra-types';

export interface ExtractionState {
  resultContent?: string;
  sourceLines?: number[];
  actualStartLine?: number;
  actualEndLine?: number;
  matchRanges?: Array<{ start: number; end: number }>;
  /** Source lines containing matches before pagination. */
  matchedLines?: number[];
  selectedMatchCount?: number;
  warnings?: string[];
  earlyResult?: LocalFetchToolResult;
}
