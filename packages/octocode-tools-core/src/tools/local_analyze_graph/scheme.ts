import { z } from 'zod';

import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import { LocalAnalyzeGraphQuerySchema } from '../../toolContract/schemas.js';

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
  status?: 'empty' | 'error';
  error?: string;
  errorCode?: string;
  rawResponseChars?: number;
  operation?: GraphOperation;
  path?: string;
  filesScanned?: number;
  filesSkipped?: number;
  results?: Array<Record<string, unknown>>;
  summary?: Record<string, unknown>;
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
  warnings?: string[];
  confidence?: 'low';
  [key: string]: unknown;
}

// Internal compatibility type while dead-code policy remains in its existing
// module. It is not exported as a public tool or schema.
export type FindDeadCodeQuery =
  Extract<AnalyzeGraphQuery, { operation: 'deadCode' }> extends infer T
    ? T extends object
      ? Omit<T, 'operation'>
      : never
    : never;

export interface FindDeadCodeOutput {
  status?: 'empty' | 'error';
  error?: string;
  errorCode?: string;
  rawResponseChars?: number;
  path?: string;
  filesScanned?: number;
  filesSkipped?: number;
  entrypointsResolved?: string[];
  deadExports?: DeadExportOutput[];
  deadClusters?: DeadClusterOutput[];
  pagination?: LocalItemPagination;
  next?: Record<string, ToolContinuation>;
  confidence?: 'low';
  warnings?: string[];
  [key: string]: unknown;
}
