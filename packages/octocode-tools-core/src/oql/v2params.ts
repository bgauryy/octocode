/**
 * Typed V2-target `params` schemas.
 *
 * V2 research targets (semantics/repositories/packages/pullRequests/commits/
 * artifacts/diff/research) carry a `params` bag that the backing tool validates
 * exhaustively. These schemas type the *documented, commonly-used* fields so a
 * type mistake (e.g. `prNumber:"abc"`) is caught at the OQL layer with a clear
 * `invalidQuery` instead of failing opaquely at the tool — while `.passthrough()`
 * keeps the backing tool the exhaustive source of truth for the rest.
 *
 * This is the typed-contract layer for OCTOCODE_SEARCH_PARITY_CHECKLIST gap #3.
 */
import { z } from 'zod';
import type { OqlActiveTargetV1 } from './types.js';

const intMin1 = z.number().int().min(1);
const nonNegInt = z.number().int().min(0);

const semanticsParams = z
  .object({
    type: z
      .enum([
        'definition',
        'references',
        'callers',
        'callees',
        'callHierarchy',
        'hover',
        'documentSymbols',
        'typeDefinition',
        'implementation',
      ])
      .optional(),
    uri: z.string().optional(),
    symbolName: z.string().optional(),
    lineHint: intMin1.optional(),
    orderHint: z.number().int().optional(),
    includeDeclaration: z.boolean().optional(),
    depth: z.number().int().min(0).max(20).optional(),
    groupByFile: z.boolean().optional(),
    workspaceRoot: z.string().optional(),
    format: z.enum(['structured', 'compact']).optional(),
    page: intMin1.optional(),
    itemsPerPage: intMin1.optional(),
  })
  .passthrough();

const repositoriesParams = z
  .object({
    keywords: z.array(z.string()).optional(),
    topicsToSearch: z.array(z.string()).optional(),
    language: z.string().optional(),
    owner: z.string().optional(),
    stars: z.union([z.string(), z.number()]).optional(),
    size: z.string().optional(),
    updated: z.string().optional(),
    license: z.string().optional(),
    visibility: z.enum(['public', 'private']).optional(),
    archived: z.boolean().optional(),
    sort: z
      .enum(['stars', 'forks', 'help-wanted-issues', 'updated', 'best-match'])
      .optional(),
    concise: z.boolean().optional(),
    page: intMin1.optional(),
  })
  .passthrough();

const packagesParams = z
  .object({
    packageName: z.string().optional(),
    keywords: z.array(z.string()).optional(),
    mode: z.enum(['lean', 'full']).optional(),
    page: intMin1.optional(),
  })
  .passthrough();

const pullRequestsParams = z
  .object({
    prNumber: intMin1.optional(),
    state: z.enum(['open', 'closed', 'merged']).optional(),
    author: z.string().optional(),
    label: z.union([z.string(), z.array(z.string())]).optional(),
    keywordsToSearch: z.array(z.string()).optional(),
    head: z.string().optional(),
    base: z.string().optional(),
    reviewMode: z.string().optional(),
    filePage: intMin1.optional(),
    commentPage: intMin1.optional(),
    commitPage: intMin1.optional(),
    charOffset: nonNegInt.optional(),
    charLength: intMin1.optional(),
    minify: z.enum(['none', 'standard']).optional(),
    limit: intMin1.optional(),
    page: intMin1.optional(),
  })
  .passthrough();

const commitsParams = z
  .object({
    path: z.string().optional(),
    branch: z.string().optional(),
    since: z.string().optional(),
    until: z.string().optional(),
    includeDiff: z.boolean().optional(),
    limit: intMin1.optional(),
    page: intMin1.optional(),
  })
  .passthrough();

const artifactsParams = z
  .object({
    mode: z
      .enum(['inspect', 'list', 'extract', 'decompress', 'strings', 'unpack'])
      .optional(),
    archiveFile: z.string().optional(),
    entryPageNumber: intMin1.optional(),
    entriesPerPage: intMin1.optional(),
    minLength: z.number().int().min(1).max(128).optional(),
    scanOffset: nonNegInt.optional(),
    charOffset: nonNegInt.optional(),
    charLength: intMin1.optional(),
    matchString: z.string().optional(),
    verbose: z.boolean().optional(),
  })
  .passthrough();

const diffParams = z
  .object({
    prNumber: intMin1.optional(),
    files: z.array(z.string()).optional(),
    baseRef: z.string().optional(),
    headRef: z.string().optional(),
    path: z.string().optional(),
  })
  .passthrough();

const researchParams = z
  .object({
    goal: z.string().optional(),
    intent: z
      .enum(['general', 'reachability', 'dependencies', 'symbols'])
      .optional(),
    facets: z.array(z.string()).optional(),
    mode: z.enum(['plan', 'analyze']).optional(),
    maxFiles: intMin1.optional(),
  })
  .passthrough();

/** Per-target params schema; targets without a `params` bag are absent. */
export const V2_PARAM_SCHEMAS: Partial<
  Record<OqlActiveTargetV1, z.ZodTypeAny>
> = {
  semantics: semanticsParams,
  repositories: repositoriesParams,
  packages: packagesParams,
  pullRequests: pullRequestsParams,
  commits: commitsParams,
  artifacts: artifactsParams,
  diff: diffParams,
  research: researchParams,
};

/**
 * Validate a target's `params` bag against its typed schema. Returns `null` when
 * valid (or no schema applies), or a Zod-style error message string the caller
 * raises as `invalidQuery`.
 */
export function validateV2Params(
  target: OqlActiveTargetV1,
  params: unknown
): string | null {
  const schema = V2_PARAM_SCHEMAS[target];
  if (!schema || params === undefined) return null;
  const parsed = schema.safeParse(params);
  if (parsed.success) return null;
  return parsed.error.issues
    .map(i => `params.${i.path.join('.') || '(root)'}: ${i.message}`)
    .join('; ');
}
