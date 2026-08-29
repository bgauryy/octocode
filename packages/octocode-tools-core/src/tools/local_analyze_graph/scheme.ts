import { z } from 'zod';

import { LOCAL_MAX_FILES_PER_PAGE } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';

const operation = <T extends string>(value: T, description: string) =>
  z.literal(value).describe(description);

const metaFields = {
  id: z.string().optional().describe('Stable id for matching batched results.'),
  researchGoal: z
    .string()
    .optional()
    .describe('The larger research goal this query advances.'),
  reasoning: z
    .string()
    .optional()
    .describe('Why this graph operation is the next useful step.'),
} as const;

const commonFields = {
  ...metaFields,
  path: z.string().describe('Absolute repository or package root to scan.'),
  excludeDir: z
    .array(z.string())
    .optional()
    .describe('Directory names to prune from graph construction.'),
  maxFiles: clampedInt(1, 50_000)
    .optional()
    .describe('Maximum source files scanned; truncation is reported.'),
  limit: clampedInt(1, 5_000)
    .optional()
    .describe('Result cap applied before pagination.'),
  page: relaxedPageNumberField.describe('Result page, 1-based.'),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE)
    .optional()
    .describe('Results returned per page.'),
} as const;

const entrypointFields = {
  entrypoints: z
    .array(z.string())
    .optional()
    .describe(
      'Repo-relative reachability roots; omit to detect package.json main/exports/bin.'
    ),
  includeTests: z
    .boolean()
    .optional()
    .default(true)
    .describe('Treat tests as reachability roots (default true).'),
} as const;

const traversalFields = {
  file: z.string().describe('Repo-relative source file.'),
  depth: clampedInt(1, 50)
    .optional()
    .default(1)
    .describe('Maximum traversal depth (default 1).'),
} as const;

export const LocalAnalyzeGraphQuerySchema = z.discriminatedUnion('operation', [
  z.object({
    ...commonFields,
    ...entrypointFields,
    operation: operation(
      'deadCode',
      'Find unreachable or unretained exported symbols and dead SCCs.'
    ),
  }),
  z.object({
    ...commonFields,
    operation: operation('cycles', 'Find strongly connected file components.'),
  }),
  z.object({
    ...commonFields,
    ...traversalFields,
    operation: operation(
      'dependencies',
      'Traverse files imported or re-exported by the source file; results include edgeKinds and syntactic confidence.'
    ),
  }),
  z.object({
    ...commonFields,
    ...traversalFields,
    operation: operation(
      'dependents',
      'Traverse files that import or re-export the source file; results include edgeKinds and syntactic confidence.'
    ),
  }),
  z.object({
    ...commonFields,
    file: z.string().describe('Repo-relative source file.'),
    target: z.string().describe('Repo-relative destination file.'),
    operation: operation(
      'path',
      'Find the shortest directed import/re-export path with per-edge provenance.'
    ),
  }),
  z.object({
    ...commonFields,
    ...entrypointFields,
    operation: operation(
      'reachability',
      'Classify scanned files by reachability from entrypoints.'
    ),
  }),
]);

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
