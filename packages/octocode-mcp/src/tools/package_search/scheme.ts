import { z } from 'zod';
import { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import { PackageSearchOutputSchema as UpstreamPackageOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  createRelaxedBulkQuerySchema,
  DEFAULT_PAGE_SIZE,
  describeField,
  optionalMetaFields,
  relaxedPageNumberField,
  withCoreSchemaDescriptions,
} from '../../scheme/localSchemaOverlay.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';
import { STATIC_TOOL_NAMES } from '../toolNames.js';

const npmPackageQueryWithLimit = withCoreSchemaDescriptions(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  NpmPackageQuerySchema.omit({
    ecosystem: true,
    searchLimit: true,
  }).extend({
    ...optionalMetaFields,
    name: describeField(
      NpmPackageQuerySchema.shape.name,
      'Exact npm package name or npm keyword query. Output is compact and includes GitHub owner/repo, sourceRoot, entrypoints, and researchTargets when available.'
    ),
    npmFetchMetadata: describeField(
      NpmPackageQuerySchema.shape.npmFetchMetadata,
      'Fetch heavier npm metadata when needed; response still summarizes descriptions and exposes research handoff fields instead of dumping dependency trees.'
    ),
    page: relaxedPageNumberField.describe(
      `Result page (1-based). Exact package-name lookups return one canonical package; keyword searches use page to walk registry results (up to ${DEFAULT_PAGE_SIZE} per page).`
    ),
  })
);

export const PackageSearchQueryLocalSchema = npmPackageQueryWithLimit;

const packageQueryWithEcosystemDefault = z.preprocess(val => {
  if (val && typeof val === 'object') {
    const record = val as Record<string, unknown>;
    const next = { ...record };
    if (
      !Object.prototype.hasOwnProperty.call(next, 'name') &&
      typeof next.packageName === 'string'
    ) {
      next.name = next.packageName;
    }
    return next;
  }
  return val;
}, PackageSearchQueryLocalSchema);

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  STATIC_TOOL_NAMES.PACKAGE_SEARCH,
  packageQueryWithEcosystemDefault,
  { maxQueries: 5 }
);

export const PackageSearchOutputLocalSchema = UpstreamPackageOutput.extend(
  responseEnvelopeFields
);
