import { z } from 'zod';
import { NpmPackageQuerySchema } from '../../toolContract/input/resources/tools/npmSearch.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import {
  getRequiredSchemaField,
  getSchemaField,
} from '../../scheme/conditionalSchemas.js';
import type {
  ItemPagination,
  ToolContinuation,
} from '../../scheme/pagination.js';
import type { BulkToolOutput } from '../../types/toolOutput.js';

const queryOverrides = {
  page: relaxedPageNumberField.removeDefault(),
  pageSize: clampedInt(1, 100)
    .optional()
    .describe('Packages returned per keyword-discovery page.'),
  keywords: z.array(z.string()).optional(),
} as const;

function requirePackageNameOrKeywords(
  query: {
    packageName?: string;
    keywords?: string | string[];
    pageSize?: number;
  },
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
  if (query.packageName?.trim() && keywords) {
    ctx.addIssue({
      code: 'custom',
      path: ['keywords'],
      message: 'provide packageName or keywords, not both',
    });
  }
  if (query.packageName?.trim() && query.pageSize !== undefined) {
    ctx.addIssue({
      code: 'custom',
      path: ['pageSize'],
      message: 'pageSize is only available for keyword discovery',
    });
  }
}

export const NpmSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
).superRefine(requirePackageNameOrKeywords);

const NpmSearchQueryShape = createQueryShapeSchema(
  NpmPackageQuerySchema,
  queryOverrides,
  { strict: true }
);
const npmCommonShape = NpmSearchQueryShape.omit({
  packageName: true,
  keywords: true,
  pageSize: true,
  page: true,
}).shape;
const packageNameMode = z
  .object({
    ...npmCommonShape,
    packageName: getRequiredSchemaField(
      NpmSearchQueryShape.shape,
      'packageName'
    ),
  })
  .strict()
  .superRefine((query, ctx) =>
    requirePackageNameOrKeywords(
      query as {
        packageName?: string;
        keywords?: string | string[];
        pageSize?: number;
      },
      ctx
    )
  );
const keywordMode = z
  .object({
    ...npmCommonShape,
    keywords: z
      .array(z.string())
      .min(1)
      .describe(
        getSchemaField(NpmSearchQueryShape.shape, 'keywords').description ?? ''
      ),
    pageSize: getSchemaField(NpmSearchQueryShape.shape, 'pageSize'),
    page: getSchemaField(NpmSearchQueryShape.shape, 'page'),
  })
  .strict()
  .superRefine((query, ctx) =>
    requirePackageNameOrKeywords(
      query as {
        packageName?: string;
        keywords?: string | string[];
        pageSize?: number;
      },
      ctx
    )
  );

export const NpmSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  z.union([packageNameMode, keywordMode]),
  { maxQueries: 5 }
);

// ---------------------------------------------------------------------------
// Output TYPES — describes what npmSearch returns. No zod: the MCP server
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

export type NpmSearchPagination = ItemPagination & {
  continuationUnavailable?: {
    reason: 'schemaPageLimit';
    maxPage: number;
  };
};

export interface NpmSearchData {
  packages?: NpmSearchPackage[];
  repositories?: Record<string, NpmSearchRepository>;
  pagination?: NpmSearchPagination;
  next?: Record<string, ToolContinuation>;
  [key: string]: unknown;
}

export type NpmSearchOutputLocal = BulkToolOutput<NpmSearchData>;
