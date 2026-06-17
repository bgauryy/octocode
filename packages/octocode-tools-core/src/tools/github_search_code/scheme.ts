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
            id: z.string(),
            owner: z.string(),
            repo: z.string(),
            path: z.string(),
            queryId: z.string().optional(),
            matches: z.array(
              z.object({
                value: z.string().optional(),
                pathOnly: z.boolean().optional(),
                matchIndices: z
                  .array(z.object({ start: z.number(), end: z.number() }))
                  .optional(),
                url: z.string().optional(),
              })
            ),
          })
        ),
        pagination: z
          .object({
            currentPage: z.number(),
            totalPages: z.number(),
            perPage: z.number(),
            totalMatches: z.number(),
            reportedTotalMatches: z.number().optional(),
            reachableTotalMatches: z.number().optional(),
            totalMatchesKind: z
              .enum(['exact', 'reported', 'lowerBound'])
              .optional(),
            totalMatchesCapped: z.boolean().optional(),
            hasMore: z.boolean(),
            uniqueFileCount: z.number().optional(),
          })
          .optional(),
      }),
    })
  ),
  hints: z.array(z.string()).optional(),
  emptyQueries: z
    .array(
      z.object({
        id: z.string(),
        hints: z.array(z.string()).optional(),
        nonExistentScope: z.literal(true).optional(),
      })
    )
    .optional(),
  errors: z
    .array(
      z.object({
        id: z.string(),
        error: z.string(),
        hints: z.array(z.string()).optional(),
      })
    )
    .optional(),
});

export type GitHubCodeSearchOutputLocal = z.infer<
  typeof GitHubCodeSearchOutputLocalSchema
>;
