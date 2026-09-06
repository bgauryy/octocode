import { z } from 'zod';

import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import { LocalAnalyzeGraphQuerySchema } from '../../toolContract/input/resources/tools/localAnalyzeGraph.js';
import type { GraphCoverage } from '../../graph/types.js';

export { LocalAnalyzeGraphQuerySchema };

export type AnalyzeGraphQuery = z.infer<typeof LocalAnalyzeGraphQuerySchema>;

export const LocalAnalyzeGraphBulkQuerySchema = createRelaxedBulkQuerySchema(
  LocalAnalyzeGraphQuerySchema,
  { maxQueries: 5 }
);

export type GraphOperation = AnalyzeGraphQuery['operation'];

export type DeadCodeReason =
  'unreachable-file' | 'unreferenced-export' | 'dead-cluster';

export interface DeadExportOutput {
  file: string;
  name: string;
  kind: string;
  line: number;
  reason: DeadCodeReason;
  clusterId?: number;
  viaHeuristic?: 'lexical-count' | 'reexport-chain';
}

export interface DeadClusterOutput {
  id: number;
  files: string[];
  reason: string;
}

export interface AnalyzeGraphOutput {
  coverage?: GraphCoverage;
  status?: 'empty' | 'error';
  error?: string;
  errorCode?: string;
  rawResponseChars?: number;
  operation?: GraphOperation;
  path?: string;
  filesScanned?: number;
  filesSkipped?: number;
  truncated?: boolean;
  terminalLimit?: boolean;
  completeness?: {
    results: 'complete' | 'pageable' | 'truncated';
    graph: 'complete' | 'scan-truncated' | 'coverage-incomplete';
    diagnostics: 'complete' | 'pageable' | 'truncated';
    coverageGapReasons?: Array<
      'parseRecovery' | 'unresolvedImports' | 'unsupportedLinking'
    >;
  };
  partialReasons?: Array<
    | 'maxFiles'
    | 'limit'
    | 'filesSkipped'
    | 'parseRecovery'
    | 'unresolvedImports'
    | 'unsupportedLinking'
    | 'diagnosticPage'
  >;
  totalAvailable?: number;
  results?: Array<Record<string, unknown>>;
  summary?: Record<string, unknown>;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
  warnings?: string[];
  confidence?: 'low';
  [key: string]: unknown;
}
