import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { GITHUB_SEARCH_MAX_LIMIT } from '../../config.js';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  EvidenceSchema,
  responseEnvelopeFields,
} from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE]?.schema,
} as Record<string, string>;

const GitHubCodeSearchQuerySchema = z.object({
  id: z.string().optional().describe(QUERY_DESCRIPTIONS.id!),
  mainResearchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.mainResearchGoal!),
  researchGoal: z
    .string()
    .optional()
    .describe(QUERY_DESCRIPTIONS.researchGoal!),
  reasoning: z.string().optional().describe(QUERY_DESCRIPTIONS.reasoning!),
  keywordsToSearch: z
    .array(z.string())
    .optional()
    .describe(QUERY_DESCRIPTIONS.keywordsToSearch!),
  owner: z.string().optional().describe(QUERY_DESCRIPTIONS.owner!),
  repo: z.string().optional().describe(QUERY_DESCRIPTIONS.repo!),
  extension: z.string().optional().describe(QUERY_DESCRIPTIONS.extension!),
  filename: z.string().optional().describe(QUERY_DESCRIPTIONS.filename!),
  path: z.string().optional().describe(QUERY_DESCRIPTIONS.path!),
  match: z
    .enum(['file', 'path'])
    .optional()
    .describe(QUERY_DESCRIPTIONS.match!),
  limit: clampedInt(1, GITHUB_SEARCH_MAX_LIMIT)
    .optional()
    .describe(QUERY_DESCRIPTIONS.limit!),
  page: relaxedPageNumberField.default(1).describe(QUERY_DESCRIPTIONS.page!),
  verbose: z.boolean().optional().describe(QUERY_DESCRIPTIONS.verbose!),
});

export const GitHubCodeSearchQueryLocalSchema = GitHubCodeSearchQuerySchema;

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(GitHubCodeSearchQuerySchema);

export const GitHubCodeSearchOutputLocalSchema = z.object({
  base: z.string().optional(),
  shared: z
    .record(z.string(), z.union([z.string(), z.number(), z.boolean()]))
    .optional(),
  evidence: EvidenceSchema,
  responsePagination: responseEnvelopeFields.responsePagination,
  results: z.array(
    z.object({
      id: z.string(),
      queryId: z.string().optional(),
      owner: z.string(),
      repo: z.string(),
      matches: z.array(
        z.object({
          path: z.string(),
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
      totalMatchesKind: z.enum(['exact', 'reported', 'lowerBound']).optional(),
      totalMatchesCapped: z.boolean().optional(),
      hasMore: z.boolean(),
      uniqueFileCount: z.number().optional(),
    })
    .optional(),
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
