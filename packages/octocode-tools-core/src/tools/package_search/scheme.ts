import { z } from 'zod';
import { NpmPackageQuerySchema } from '../../toolContract/schemas.js';
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
  // Sibling tools take keyword ARRAYS
  // (ghSearchCode/localSearchCode) — agents reflexively pass arrays here too.
  // Accept both shapes; execution folds arrays to the space-joined registry
  // query (no zod transform — it would break JSON-schema generation).
  keywords: z.union([z.string(), z.array(z.string())]).optional(),
} as const;

function requirePackageNameOrKeywords(
  query: { packageName?: string; keywords?: string | string[] },
  ctx: z.RefinementCtx
): void {
  const keywords = Array.isArray(query.keywords)
    ? query.keywords.join(' ').trim()
    : query.keywords?.trim();
  if (!query.packageName?.trim() && !keywords) {
    ctx.addIssue({
      code: 'custom',
      path: ['packageName'],
      message: 'provide packageName or keywords',
    });
  }
}

export const NpmSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
).superRefine(requirePackageNameOrKeywords);

export const NpmSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(NpmPackageQuerySchema, queryOverrides, {
    strict: true,
  }).superRefine(requirePackageNameOrKeywords),
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
