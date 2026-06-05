/**
 * Zod schemas for npm CLI output validation.
 *
 * Validates parsed JSON from npm view/search/deprecation commands
 * before type-asserting to internal interfaces.
 */

import { z } from 'zod';

/**
 * Schema for `npm view <pkg> --json` output.
 * Permissive: requires name+version, rest is optional.
 */
export const NpmViewResultSchema = z.looseObject({
  name: z.string(),
  version: z.string(),
  repository: z
    .union([
      z.string(),
      z.object({ url: z.string().optional(), type: z.string().optional() }),
    ])
    .optional(),
  main: z.string().optional(),
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

/**
 * Schema for a single result item from the npm registry search API
 * `GET https://registry.npmjs.org/-/v1/search?text=<query>&size=<n>`
 *
 * Uses z.looseObject() so extra fields (score, searchScore, date, keywords,
 * publisher, maintainers, etc.) are silently passed through rather than
 * causing a parse failure.
 * All string fields use .nullish() because real registry responses can return
 * null for name/version/description on unpublished or deprecated packages.
 * Items with null names are filtered out in npm.ts after validation.
 */
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
});

/**
 * Schema for the full response from the npm registry search API.
 * total may be a number or a string depending on registry implementation.
 */
export const NpmRegistrySearchSchema = z.looseObject({
  objects: z.array(NpmRegistrySearchItemSchema),
  total: z.union([z.number(), z.string()]).optional(),
});

/**
 * Schema for `npm view <pkg> deprecated --json` output.
 * Can be a string message or other JSON value.
 */
export const NpmDeprecationOutputSchema = z.union([
  z.string(),
  z.boolean(),
  z.number(),
  z.null(),
  z.record(z.string(), z.unknown()),
]);
