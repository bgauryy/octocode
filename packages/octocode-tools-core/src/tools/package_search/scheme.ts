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
import { ItemPaginationSchema } from '../../scheme/pagination.js';

const queryOverrides = {
  page: relaxedPageNumberField,
  // Accepted because `search --target packages --mode lean|full` sends it.
  // The strict npm bulk schema would otherwise reject it as an unrecognized key.
  // Execution currently no-ops it, but the field must stay part of the contract.
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
                      downloads: z.number().optional(),
                      repository: z.string().optional(),
                      repositoryDirectory: z.string().optional(),
                      repositoryId: z.string().optional(),
                      next: z.record(z.string(), z.unknown()).optional(),
                    })
                    .passthrough()
                )
                .optional(),
              repositories: z
                .record(
                  z.string(),
                  z
                    .object({
                      repository: z.string(),
                      owner: z.string(),
                      repo: z.string(),
                      repositoryDirectory: z.string().optional(),
                      next: z.record(z.string(), z.unknown()),
                    })
                    .passthrough()
                )
                .optional(),
              pagination: ItemPaginationSchema.optional(),
            })
            .optional(),
          status: z.string().optional(),
        })
      )
      .optional(),
  })
  .extend(responseEnvelopeFields);
