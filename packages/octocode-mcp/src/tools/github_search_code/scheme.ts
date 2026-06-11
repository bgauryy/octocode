import { z } from 'zod';
import { GitHubCodeSearchQuerySchema as UpstreamGitHubCodeSearchQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import {
  EvidenceSchema,
  responseEnvelopeFields,
} from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

export const GitHubCodeSearchQueryLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
  UpstreamGitHubCodeSearchQuerySchema.omit({ limit: true }).extend({
    ...optionalMetaFields,
    page: z.number().int().min(1).max(1000).optional().default(1),
  })
);

export const GitHubCodeSearchBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_SEARCH_CODE,
    GitHubCodeSearchQueryLocalSchema
  );

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
          path: z.string().describe('Repo-relative file path of the match.'),
          value: z
            .string()
            .optional()
            .describe(
              'Code snippet returned by GitHub for this match. NOT the full file — use githubGetFileContent to read the full file.'
            ),
          pathOnly: z
            .boolean()
            .optional()
            .describe(
              'True when GitHub returned a path match but no text snippet. Use githubGetFileContent with matchString to inspect content.'
            ),
          matchIndices: z
            .array(
              z
                .object({ start: z.number(), end: z.number() })
                .describe(
                  'Character offsets within the `value` snippet string (not line numbers in the file).'
                )
            )
            .optional()
            .describe(
              'Character-offset spans inside the `value` snippet that highlight the matched terms. These are NOT line numbers — use githubGetFileContent with matchString to get exact line positions.'
            ),
          url: z
            .string()
            .optional()
            .describe('verbose mode: html URL of the matched file.'),
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
      hasMore: z.boolean(),
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
