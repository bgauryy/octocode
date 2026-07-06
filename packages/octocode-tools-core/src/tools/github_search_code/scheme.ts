import { z } from 'zod';
import { GitHubCodeSearchQuerySchema as CoreGitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { ItemPaginationSchema } from '../../scheme/pagination.js';

const queryOverrides = {
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT).optional(),
  page: relaxedPageNumberField.default(1),
} as const;

export const GitHubCodeSearchQueryLocalSchema = describeQuerySchema(
  CoreGitHubCodeSearchQuerySchema,
  queryOverrides
);

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(CoreGitHubCodeSearchQuerySchema, queryOverrides)
  );

// Search-specific pagination: extends the canonical base with fields that are
// semantically unique to code-search (not aliases for existing canonical fields).
const CodeSearchPaginationSchema = ItemPaginationSchema.extend({
  totalMatchesKind: z.enum(['exact', 'reported', 'lowerBound']).optional(),
  totalMatchesCapped: z.boolean().optional(),
  uniqueFileCount: z.number().optional(),
});

export const GitHubCodeSearchOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      data: z.object({
        files: z.array(
          z.object({
            owner: z.string(),
            repo: z.string(),
            path: z.string(),
            queryId: z.string().optional(),
            matches: z.array(
              z.object({
                value: z.string().optional(),
                pathOnly: z.boolean().optional(),
                matchIndices: z
                  .array(
                    z.object({
                      start: z.number(),
                      end: z.number(),
                      lineOffset: z.number(),
                    })
                  )
                  .optional(),
                url: z.string().optional(),
              })
            ),
          })
        ),
        pagination: CodeSearchPaginationSchema.optional(),
      }),
    })
  ),
  emptyQueries: z
    .array(
      z.object({
        id: z.string(),
        nonExistentScope: z.literal(true).optional(),
        incompleteResults: z.literal(true).optional(),
      })
    )
    .optional(),
  warnings: z.array(z.string()).optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        error: z.string(),
      })
    )
    .optional(),
});

export type GitHubCodeSearchOutputLocal = z.infer<
  typeof GitHubCodeSearchOutputLocalSchema
>;
