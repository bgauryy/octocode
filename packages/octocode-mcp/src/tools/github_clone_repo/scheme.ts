import { z } from 'zod';
import { BulkCloneRepoSchema as UpstreamBulkCloneRepoSchema } from '@octocodeai/octocode-core/schemas';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  createRelaxedBulkQuerySchema,
  optionalMetaFields,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const CloneRepoElementSchema = (
  UpstreamBulkCloneRepoSchema.shape.queries as z.ZodArray<z.ZodTypeAny>
).element as unknown as z.ZodObject<z.ZodRawShape>;

export const CloneRepoQueryLocalSchema = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  CloneRepoElementSchema.omit({ sparse_path: true } as Record<
    string,
    true
  >).extend({
    ...optionalMetaFields,
    sparsePath: z
      .string()
      .optional()
      .describe(
        'Optional subdirectory for sparse checkout — reduces clone size for large monorepos. Use "packages/foo" to clone only that subtree.'
      ),
  })
);

export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.GITHUB_CLONE_REPO,
  CloneRepoQueryLocalSchema
);

export const GitHubCloneRepoOutputLocalSchema = UpstreamCloneRepoOutput.extend(
  responseEnvelopeFields
);
