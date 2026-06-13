import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import {
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const QUERY_DESCRIPTIONS = {
  ...completeMetadata.baseSchema,
  ...completeMetadata.tools[STATIC_TOOL_NAMES.PACKAGE_SEARCH]?.schema,
} as Record<string, string>;

const PackageSearchQueryShape = z.object({
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
  packageName: z.string().describe(QUERY_DESCRIPTIONS.packageName!),
  page: relaxedPageNumberField.describe(QUERY_DESCRIPTIONS.page!),
});

export const PackageSearchQueryLocalSchema = PackageSearchQueryShape;

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  PackageSearchQueryShape,
  { maxQueries: 5 }
);

/**
 * Output: each result's `packages` is a list of strings in the form:
 *   "name repoUrl[ sourceRoot]"
 * e.g.
 *   "zod https://github.com/colinhacks/zod"
 *   "react https://github.com/facebook/react packages/react"
 */
export const PackageSearchOutputLocalSchema = z
  .object({
    results: z
      .array(
        z.looseObject({
          id: z.string(),
          data: z
            .looseObject({
              packages: z.array(z.string()),
              totalFound: z.number().optional(),
            })
            .optional(),
          status: z.string().optional(),
        })
      )
      .optional(),
  })
  .extend(responseEnvelopeFields);
