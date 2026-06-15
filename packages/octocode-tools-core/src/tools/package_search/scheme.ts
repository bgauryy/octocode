import { z } from 'zod';
import { NpmPackageQuerySchema } from '@octocodeai/octocode-core/schemas';
import {
  createRelaxedBulkQuerySchema,
  relaxedPageNumberField,
} from '../../scheme/fields.js';
import {
  createQueryShapeSchema,
  describeQuerySchema,
} from '../../scheme/coreSchemas.js';
import { responseEnvelopeFields } from '../../scheme/responseEnvelope.js';

const queryOverrides = {
  page: relaxedPageNumberField,
} as const;

export const PackageSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
);

export const PackageSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(NpmPackageQuerySchema, queryOverrides),
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
              pagination: z
                .object({
                  currentPage: z.number(),
                  totalPages: z.number(),
                  perPage: z.number(),
                  totalFound: z.number(),
                  returned: z.number(),
                  hasMore: z.boolean(),
                })
                .optional(),
            })
            .optional(),
          status: z.string().optional(),
        })
      )
      .optional(),
  })
  .extend(responseEnvelopeFields);
