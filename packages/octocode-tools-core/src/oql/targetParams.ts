/**
 * Typed target-specific `params` schemas.
 *
 * Research targets (semantics/repositories/packages/pullRequests/commits/
 * diff/research/graph) carry a `params` bag that the backing tool validates
 * exhaustively. These schemas type the documented common fields so a type
 * mistake (e.g. `prNumber:"abc"`) is caught at the OQL layer with a clear
 * `invalidQuery` instead of failing opaquely at the tool, while `.passthrough()`
 * keeps the backing tool the source of truth for the rest.
 */
import { z } from 'zod';
import type { OqlActiveTarget } from './types.js';

const intMin1 = z.number().int().min(1);
const nonNegInt = z.number().int().min(0);
const researchFacet = z.enum(['symbols', 'files', 'dependencies', 'relations']);

const codeParams = z
  .object({
    // `match:"path"` locates files by name (no snippets, far cheaper);
    // `match:"file"` (default) reads snippets. `concise:true` flattens to
    // "owner/repo:path" strings. Both are ghSearchCode smart controls an agent
    // should be able to toggle via OQL — previously untyped on the `code` target.
    match: z.enum(['file', 'path']).optional(),
    concise: z.boolean().optional(),
    extension: z.string().optional(),
    filename: z.string().optional(),
    page: intMin1.optional(),
    limit: intMin1.optional(),
  })
  .passthrough();

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
        'workspaceSymbol',
        'supertypes',
        'subtypes',
        'diagnostic',
      ])
      .optional(),
    uri: z.string().optional(),
    symbolName: z.string().optional(),
    symbolKind: z.string().optional(),
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
    topicsToSearch: z
      .union([z.string(), z.array(z.string())])
      .transform(v => (typeof v === 'string' ? [v] : v))
      .optional(),
    language: z.string().optional(),
    owner: z.string().optional(),
    stars: z.union([z.string(), z.number()]).optional(),
    forks: z.string().optional(),
    goodFirstIssues: z.string().optional(),
    size: z.string().optional(),
    created: z.string().optional(),
    updated: z.string().optional(),
    license: z.string().optional(),
    match: z.array(z.enum(['name', 'description', 'readme'])).optional(),
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
    concise: z.boolean().optional(),
    state: z.enum(['open', 'closed', 'merged']).optional(),
    author: z.string().optional(),
    label: z.union([z.string(), z.array(z.string())]).optional(),
    keywordsToSearch: z
      .union([z.string(), z.array(z.string())])
      .transform(v => (typeof v === 'string' ? [v] : v))
      .optional(),
    head: z.string().optional(),
    base: z.string().optional(),
    created: z.string().optional(),
    updated: z.string().optional(),
    closed: z.string().optional(),
    'merged-at': z.string().optional(),
    draft: z.boolean().optional(),
    archived: z.boolean().optional(),
    sort: z
      .enum(['created', 'updated', 'best-match', 'comments', 'reactions'])
      .optional(),
    order: z.enum(['asc', 'desc']).optional(),
    reviewMode: z.string().optional(),
    filePage: intMin1.optional(),
    commentPage: intMin1.optional(),
    commitPage: intMin1.optional(),
    charOffset: nonNegInt.optional(),
    charLength: intMin1.optional(),
    minify: z.enum(['none', 'standard']).optional(),
    limit: intMin1.optional(),
    page: intMin1.optional(),
    // A *content* filter (not a search-index query): after PRs are fetched,
    // keep only those whose `matchScope` text contains `matchString`
    // (case-insensitive). Defaults to the PR body. Absent matches → zeroMatches.
    matchString: z.string().optional(),
    matchScope: z
      .enum(['body', 'title', 'comments', 'reviews', 'all'])
      .optional(),
    content: z.record(z.string(), z.unknown()).optional(),
  })
  .passthrough();

const commitsParams = z
  .object({
    path: z.string().optional(),
    branch: z.string().optional(),
    since: z.string().optional(),
    until: z.string().optional(),
    author: z.string().optional(),
    includeDiff: z.boolean().optional(),
    limit: intMin1.optional(),
    page: intMin1.optional(),
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
    facets: z.array(researchFacet).optional(),
    // For target:"research", "prove" requires explicit intent but does not run
    // LSP reference proof internally (results stay candidate-grade). The research
    // row carries a one-call `next.graph` upgrade — a pre-filled target:"graph"
    // proof:"lsp" query, page-aligned and bounded by proofLimit — so a single
    // follow-up run escalates the current page's candidates to LSP-proven facts.
    mode: z.enum(['plan', 'analyze', 'prove']).optional(),
    maxFiles: intMin1.optional(),
  })
  .passthrough();

const graphParams = z
  .object({
    goal: z.string().optional(),
    intent: z
      .enum(['general', 'reachability', 'dependencies', 'symbols'])
      .optional(),
    facets: z.array(researchFacet).optional(),
    mode: z.enum(['plan', 'analyze', 'prove']).optional(),
    maxFiles: intMin1.optional(),
    subject: z.string().optional(),
    subjectKind: z
      .enum([
        'file',
        'symbol',
        'function',
        'class',
        'method',
        'interface',
        'type',
        'dependency',
        'package',
        'entrypoint',
      ])
      .optional(),
    relation: z.union([z.string(), z.array(z.string())]).optional(),
    verdict: z.union([z.string(), z.array(z.string())]).optional(),
    direction: z.enum(['incoming', 'outgoing', 'both']).optional(),
    proof: z.enum(['none', 'lsp']).optional(),
    proofLimit: intMin1.max(25).optional(),
    includePackets: z.boolean().optional(),
    includeFacts: z.boolean().optional(),
    includeEdges: z.boolean().optional(),
  })
  .passthrough();

/** Per-target params schema; targets without a `params` bag are absent. */
const TARGET_PARAM_SCHEMAS: Partial<Record<OqlActiveTarget, z.ZodTypeAny>> = {
  semantics: semanticsParams,
  repositories: repositoriesParams,
  packages: packagesParams,
  pullRequests: pullRequestsParams,
  commits: commitsParams,
  diff: diffParams,
  research: researchParams,
  graph: graphParams,
  code: codeParams,
};

/**
 * Validate a target's `params` bag against its typed schema. Returns `null` when
 * valid (or no schema applies), or a Zod-style error message string the caller
 * raises as `invalidQuery`.
 */
export function validateTargetParams(
  target: OqlActiveTarget,
  params: unknown
): string | null {
  const schema = TARGET_PARAM_SCHEMAS[target];
  if (!schema || params === undefined) return null;
  const parsed = schema.safeParse(params);
  if (parsed.success) return null;
  return parsed.error.issues
    .map(i => `params.${i.path.join('.') || '(root)'}: ${i.message}`)
    .join('; ');
}
