import { z } from 'zod';
import { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import type {
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  page: relaxedPageNumberField,
  // The strict npm bulk schema would otherwise reject it as an unrecognized key.
  // Execution currently no-ops it, but the field must stay part of the contract.
  // Core types this as a single string, but sibling tools take keyword ARRAYS
  // (ghSearchCode/localSearchCode) — agents reflexively pass arrays here too.
  // Accept both shapes; execution folds arrays to the space-joined registry
  // query (no zod transform — it would break JSON-schema generation).
  keywords: z
    .union([z.string(), z.array(z.string())])
    .optional()
    .describe(
      'Registry keyword query (string; an array of terms is accepted and joined with spaces).'
    ),
} as const;

export const NpmSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
);

export const NpmSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(NpmPackageQuerySchema, queryOverrides, {
    strict: true,
  }),
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what packageSearch returns. No zod: the MCP server
// registers no outputSchema. Index signatures mirror the original
// z.looseObject/.passthrough() for additive runtime fields. Shared envelope
// lives in types/toolOutput.ts.
// ---------------------------------------------------------------------------

export interface NpmSearchPackage {
  name: string;
  version?: string;
  description?: string;
  license?: string;
  downloads?: number;
  repository?: string;
  repositoryDirectory?: string;
  repositoryId?: string;
  next?: Record<string, ToolContinuation>;
  [key: string]: unknown;
}

export interface NpmSearchRepository {
  repository: string;
  owner: string;
  repo: string;
  repositoryDirectory?: string;
  next: Record<string, ToolContinuation>;
  [key: string]: unknown;
}

export interface NpmSearchData {
  packages?: NpmSearchPackage[];
  repositories?: Record<string, NpmSearchRepository>;
  pagination?: ItemPagination;
  [key: string]: unknown;
}

export type NpmSearchOutputLocal = BulkToolOutput<NpmSearchData>;
