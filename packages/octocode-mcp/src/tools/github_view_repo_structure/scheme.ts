import { GitHubViewRepoStructureQuerySchema as UpstreamGitHubViewRepoStructureQuerySchema } from '@octocodeai/octocode-core/schemas';
import { GitHubViewRepoStructureOutputSchema as UpstreamStructureOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  clampedInt,
  createRelaxedBulkQuerySchema,
  depthField,
  describeField,
  optionalMetaFields,
  relaxedPageNumberField,
  STRUCTURE_PAGE_SIZE,
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
      owner: describeField(
        UpstreamGitHubViewRepoStructureQuerySchema.shape.owner,
        'GitHub repository owner or organization.'
      ),
      repo: describeField(
        UpstreamGitHubViewRepoStructureQuerySchema.shape.repo,
        'GitHub repository name without the owner.'
      ),
      path: describeField(
        UpstreamGitHubViewRepoStructureQuerySchema.shape.path,
        'Repository-relative directory path to browse. Use "" or "." for the root.'
      ),
      branch: describeField(
        UpstreamGitHubViewRepoStructureQuerySchema.shape.branch,
        'Branch, tag, or commit SHA. Omit to use the repository default branch.'
      ),
      page: relaxedPageNumberField
        .default(1)
        .describe(
          `Result page (1-based). Each page returns up to ${STRUCTURE_PAGE_SIZE} entries. Use page=2, page=3, … to walk through large directories.`
        ),
      itemsPerPage: clampedInt(1, 200)
        .optional()
        .describe('Entries per page for repository structure pagination.'),
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
