import { CloneRepoQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubCloneRepoOutputSchema as UpstreamCloneRepoOutput } from '@octocodeai/octocode-core/schemas/outputs';
import { createRelaxedBulkQuerySchema } from '../../scheme/fields.js';
import { describeQuerySchema } from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';

export const CloneRepoQueryLocalSchema =
  describeQuerySchema(CloneRepoQuerySchema);

export const BulkCloneRepoLocalSchema = createRelaxedBulkQuerySchema(
  CloneRepoQueryLocalSchema
);

export const GitHubCloneRepoOutputLocalSchema = UpstreamCloneRepoOutput.extend(
  responseEnvelopeFields
);
