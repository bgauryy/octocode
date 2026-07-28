import { z } from 'zod';
import { FindDeadCodeQuerySchema as CoreFindDeadCodeQuerySchema } from '@octocodeai/octocode-core/schemas';
import { LOCAL_MAX_FILES_PER_PAGE } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  LocalItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';

const queryOverrides = {
  maxFiles: clampedInt(1, 50_000).optional(),
  limit: clampedInt(1, 5_000).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, LOCAL_MAX_FILES_PER_PAGE).optional(),
} as const;

const FindDeadCodeQueryShape = createQueryShapeSchema(
  CoreFindDeadCodeQuerySchema,
  queryOverrides
);

export type FindDeadCodeQuery = z.infer<typeof CoreFindDeadCodeQuerySchema>;

export const LocalFindDeadCodeQuerySchema = describeQuerySchema(
  CoreFindDeadCodeQuerySchema,
  queryOverrides
) as unknown as z.ZodType<FindDeadCodeQuery>;

export const LocalFindDeadCodeBulkQuerySchema = createRelaxedBulkQuerySchema(
  FindDeadCodeQueryShape,
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — no zod validation on output (matches every other local tool);
// shared envelope lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export type DeadCodeReason =
  'unreachable-file' | 'unreferenced-export' | 'dead-cluster';

export interface DeadExportOutput {
  file: string;
  name: string;
  kind: string;
  line: number;
  reason: DeadCodeReason;
  clusterId?: number;
}

export interface DeadClusterOutput {
  id: number;
  files: string[];
  reason: string;
}

// Flat per-query result (matches every other local tool's *ToolResult shape:
// fields live directly on the object, plus an index signature) — this is
// what `executeBulkOperation` wraps N of into the bulk `results[]` envelope,
// not something this tool builds itself.
export interface FindDeadCodeOutput {
  status?: 'empty' | 'error';
  warnings?: string[];
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
  [key: string]: unknown;
}
