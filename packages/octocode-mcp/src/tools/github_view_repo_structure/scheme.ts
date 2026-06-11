import { GitHubViewRepoStructureQuerySchema as UpstreamGitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  depthField,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

export const GitHubViewRepoStructureQueryLocalSchema =
  withCoreSchemaDescriptions(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    UpstreamGitHubViewRepoStructureQuerySchema.omit({
      entriesPerPage: true,
      entryPageNumber: true,
    }).extend({
      ...optionalMetaFields,
      page: relaxedPageNumberField.default(1),
      itemsPerPage: clampedInt(1, 200).optional(),
      depth: depthField,
    })
  );

export const GitHubViewRepoStructureBulkQueryLocalSchema =
  createRelaxedBulkQuerySchema(
    STATIC_TOOL_NAMES.GITHUB_VIEW_REPO_STRUCTURE,
    GitHubViewRepoStructureQueryLocalSchema
  );

export const GitHubViewRepoStructureOutputLocalSchema =
  UpstreamStructureOutput.extend(responseEnvelopeFields);
