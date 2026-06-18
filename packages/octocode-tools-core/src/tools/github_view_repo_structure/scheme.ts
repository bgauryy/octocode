import { GitHubViewRepoStructureQuerySchema as CoreGitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput } from '@octocodeai/octocode-core/schemas/outputs';
import { GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE } from '../../config.js';
import { LOCAL_MAX_DEPTH } from '../../config.js';
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

// Field set + descriptions (incl. includeSizes) come from octocode-core; the
// runtime only relaxes the numeric/pagination bounds (clamp instead of reject).
const queryOverrides = {
  maxDepth: clampedInt(0, LOCAL_MAX_DEPTH).optional(),
  page: relaxedPageNumberField.default(1),
  itemsPerPage: clampedInt(1, GITHUB_STRUCTURE_MAX_ENTRIES_PER_PAGE).optional(),
} as const;

export const GitHubViewRepoStructureQueryLocalSchema = describeQuerySchema(
  CoreGitHubViewRepoStructureQuerySchema,
  queryOverrides
);

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    createQueryShapeSchema(
      CoreGitHubViewRepoStructureQuerySchema,
      queryOverrides
    )
  );

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(responseEnvelopeFields);
