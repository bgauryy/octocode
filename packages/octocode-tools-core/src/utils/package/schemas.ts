import { z } from 'zod';

export const NpmViewResultSchema = z.looseObject({
  name: z.string(),
  version: z.string(),
  repository: z
    .union([
      z.string(),
      z.object({
        url: z.string().optional(),
        type: z.string().optional(),
        directory: z.string().optional(),
      }),
    ])
    .optional(),
  main: z.string().optional(),
  module: z.string().optional(),
  type: z.string().optional(),
  exports: z.unknown().optional(),
  types: z.string().optional(),
  typings: z.string().optional(),
  description: z.string().optional(),
  keywords: z.array(z.string()).optional(),
  license: z
    .union([z.string(), z.object({ type: z.string().optional() })])
    .optional(),
  homepage: z.string().optional(),
  author: z
    .union([
      z.string(),
      z.object({
        name: z.string().optional(),
        email: z.string().optional(),
        url: z.string().optional(),
      }),
    ])
    .optional(),
  maintainers: z
    .array(
      z.object({ name: z.string().optional(), email: z.string().optional() })
    )
    .optional(),
  engines: z.record(z.string(), z.string()).optional(),
  dependencies: z.record(z.string(), z.string()).optional(),
  devDependencies: z.record(z.string(), z.string()).optional(),
  peerDependencies: z.record(z.string(), z.string()).optional(),
  time: z.record(z.string(), z.string().optional()).optional(),
});

const NpmRegistrySearchItemSchema = z.looseObject({
  package: z.looseObject({
    name: z.string().nullish(),
    version: z.string().nullish(),
    description: z.string().nullish(),
    links: z
      .looseObject({
        npm: z.string().nullish(),
        homepage: z.string().nullish(),
        repository: z.string().nullish(),
      })
      .nullish(),
  }),
  score: z
    .looseObject({
      final: z.number().nullish(),
      detail: z
        .looseObject({
          quality: z.number().nullish(),
          popularity: z.number().nullish(),
          maintenance: z.number().nullish(),
        })
        .nullish(),
    })
    .nullish(),
});

export const NpmRegistrySearchSchema = z.looseObject({
  objects: z.array(NpmRegistrySearchItemSchema),
  total: z.union([z.number(), z.string()]).optional(),
});

export const NpmDeprecationOutputSchema = z.union([
  z.string(),
  z.boolean(),
  z.number(),
  z.null(),
  z.record(z.string(), z.unknown()),
]);
