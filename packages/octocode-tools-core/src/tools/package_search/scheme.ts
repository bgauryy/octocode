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
  mode: z.enum(['lean', 'full']).optional(),
} as const;

export const NpmSearchQueryLocalSchema = describeQuerySchema(
  NpmPackageQuerySchema,
  queryOverrides
);

export const NpmSearchBulkQueryLocalSchema = createRelaxedBulkQuerySchema(
  createQueryShapeSchema(NpmPackageQuerySchema, queryOverrides, {
    strict: true,
  }),
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
              packages: z
                .array(
                  z
                    .object({
                      name: z.string(),
                      version: z.string().optional(),
                      description: z.string().optional(),
                      license: z.string().optional(),
                      weeklyDownloads: z.number().optional(),
                      repository: z.string().optional(),
                      repositoryDirectory: z.string().optional(),
                    })
                    .passthrough()
                )
                .optional(),
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
