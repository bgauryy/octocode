import { z } from 'zod';
import { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import { PackageSearchOutputSchema as UpstreamPackageOutput } from '@octocodeai/octocode-core/schemas/outputs';
import {
  createRelaxedBulkQuerySchema,
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
    page: relaxedPageNumberField,
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
