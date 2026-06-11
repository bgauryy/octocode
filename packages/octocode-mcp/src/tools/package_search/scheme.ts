import { z } from 'zod';
import { completeMetadata } from '@octocodeai/octocode-core';
import { PackageSearchOutputSchema as UpstreamPackageOutput } from '@octocodeai/octocode-core/schemas/outputs';
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

const PACKAGE_SEARCH_MODES = ['smart', 'full', 'lean'] as const;

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
  name: z.string().describe(QUERY_DESCRIPTIONS.name!),
  mode: z
    .enum(PACKAGE_SEARCH_MODES)
    .optional()
    .describe(QUERY_DESCRIPTIONS.mode!),
  page: relaxedPageNumberField.describe(QUERY_DESCRIPTIONS.page!),
});

export const PackageSearchQueryLocalSchema = PackageSearchQueryShape;

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  PackageSearchQueryShape,
  { maxQueries: 5 }
);

export const PackageSearchOutputLocalSchema = UpstreamPackageOutput.extend(
  responseEnvelopeFields
);
