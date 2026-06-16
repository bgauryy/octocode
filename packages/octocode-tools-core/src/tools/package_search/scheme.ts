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

export const NpmSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
);

export const NpmSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(NpmPackageQuerySchema, queryOverrides),
  { maxQueries: 5 }
);

export const NpmSearchOutputLocalSchema = z
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
