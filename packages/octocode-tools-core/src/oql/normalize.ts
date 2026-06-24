/**
 * OQL normalizer: raw sugar in, strict canonical OQL out.
 *
 * Rules (see OCTOCODE_QUERY_LANGUAGE.md §normalization):
 *  - sugar is accepted only when it has a deterministic rewrite;
 *  - ambiguous sugar fails with `ambiguousSugar`;
 *  - reserved targets fail with `unsupportedTarget`;
 *  - unknown fields fail with `unknownField`;
 *  - canonical output contains no shorthand fields.
 *
 * Predicate IDs are NOT injected here — the planner derives stable IDs from
 * node position (or a user-provided `id`) so the canonical `where` stays clean.
 */
import {
  OqlInputBatchSchema,
  OqlInputQuerySchema,
  OqlQuerySchema,
  PredicateSchema,
} from './schema.js';
import { OqlValidationError, diagnostic } from './diagnostics.js';
import { validateTargetParams } from './targetParams.js';
import {
  ACTIVE_TARGETS,
  RESERVED_TARGETS,
  CORPUS_OPTIONAL_TARGETS,
  type MaterializePolicy,
  type OqlBatch,
  type OqlCanonicalInput,
  type OqlInputBatch,
  type OqlInputQuery,
  type OqlQuery,
  type OqlSearchInput,
  type Predicate,
  type QueryScope,
  type QuerySource,
  type StructuralRuleInput,
  isBatchInput,
} from './types.js';

/** Canonical + sugar keys consumed by the normalizer. */
const KNOWN_QUERY_KEYS = new Set<string>([
  // canonical
  'schema',
  'id',
  'target',
  'from',
  'scope',
  'where',
  'materialize',
  'fetch',
  'select',
  'view',
  'controls',
  'limit',
  'page',
  'itemsPerPage',
  'params',
  'explain',
  // sugar
  'repo',
  'owner',
  'ref',
  'path',
  'text',
  'regex',
  'pattern',
  'rule',
  'lang',
  'and',
  'or',
  'xor',
  'noneOf',
  'oneOf',
  'invert',
  'filesOnly',
  'filesWithoutMatch',
  // base-query meta auto-filled by interfaces; ignored by OQL
  'mainResearchGoal',
  'researchGoal',
  'reasoning',
  'verbose',
]);

function fail(...diagnostics: ReturnType<typeof diagnostic>[]): never {
  throw new OqlValidationError(diagnostics);
}

function asArray<T>(v: T | T[] | undefined): T[] | undefined {
  if (v === undefined) return undefined;
  return Array.isArray(v) ? v : [v];
}

/* ----------------------------- public API ------------------------------- */

export function normalizeInput(input: OqlSearchInput): OqlCanonicalInput {
  if (isBatchInput(input)) {
    return normalizeBatch(input);
  }
  return normalizeQuery(input as OqlInputQuery);
}

function normalizeBatch(input: OqlInputBatch): OqlBatch {
  const parsed = OqlInputBatchSchema.safeParse(input);
  if (!parsed.success) {
    fail(diagnostic('invalidQuery', formatZodError(parsed.error)));
  }
  const raw = parsed.data as OqlInputBatch;
  // Reject unknown batch-level keys (same strictness as query level — e.g.
  // `batchId` is not a field; the id field is `id`).
  const KNOWN_BATCH_KEYS = new Set([
    'schema',
    'id',
    'queries',
    'combine',
    'limit',
    'page',
    'itemsPerPage',
    'explain',
  ]);
  for (const key of Object.keys(raw)) {
    if (!KNOWN_BATCH_KEYS.has(key)) {
      fail(
        diagnostic(
          'unknownField',
          `Unknown batch field "${key}" is not part of OQL.`,
          { queryPath: key }
        )
      );
    }
  }
  if (raw.queries.length > 5) {
    fail(
      diagnostic(
        'invalidQuery',
        'OQL batches are capped at 5 queries per call.',
        { queryPath: 'queries' }
      )
    );
  }
  const queries = raw.queries.map((q, i) => {
    try {
      return normalizeQuery(q as OqlInputQuery);
    } catch (err) {
      if (err instanceof OqlValidationError) {
        // prefix queryPath with the child index for traceability
        throw new OqlValidationError(
          err.diagnostics.map(d => ({
            ...d,
            queryPath: `queries[${i}]${d.queryPath ? `.${d.queryPath}` : ''}`,
          }))
        );
      }
      throw err;
    }
  });
  return {
    schema: 'oql',
    ...(raw.id ? { id: raw.id } : {}),
    queries,
    combine: raw.combine ?? 'independent',
    ...(raw.limit !== undefined ? { limit: raw.limit } : {}),
    ...(raw.page !== undefined ? { page: raw.page } : {}),
    ...(raw.itemsPerPage !== undefined
      ? { itemsPerPage: raw.itemsPerPage }
      : {}),
    ...(raw.explain !== undefined ? { explain: raw.explain } : {}),
  };
}

export function normalizeQuery(input: OqlInputQuery): OqlQuery {
  const parsed = OqlInputQuerySchema.safeParse(input);
  if (!parsed.success) {
    fail(diagnostic('invalidQuery', formatZodError(parsed.error)));
  }
  const raw = {
    ...(parsed.data as Record<string, unknown>),
  } as OqlInputQuery;

  // 1. resolve target. `filesWithoutMatch` sugar forces "files"; otherwise use
  // the explicit target or infer it from the rest of the query.
  const target = raw.filesWithoutMatch
    ? 'files'
    : (raw.target ?? inferTarget(raw));
  if (target === undefined) {
    fail(
      diagnostic(
        'invalidQuery',
        `Could not determine \`target\`; specify one of: ${ACTIVE_TARGETS.join(', ')}.`,
        { queryPath: 'target' }
      )
    );
  }
  if ((RESERVED_TARGETS as readonly string[]).includes(target)) {
    fail(
      diagnostic(
        'unsupportedTarget',
        `Target "${target}" is reserved until proof/dry-run support exists.`,
        {
          queryPath: 'target',
          repair: {
            message: `Use an active target: ${ACTIVE_TARGETS.join(', ')}.`,
          },
        }
      )
    );
  }
  if (!(ACTIVE_TARGETS as readonly string[]).includes(target)) {
    fail(diagnostic('unknownField', `Unknown target "${target}".`));
  }

  // 2. reject unknown top-level keys
  for (const key of Object.keys(raw)) {
    if (!KNOWN_QUERY_KEYS.has(key)) {
      fail(
        diagnostic(
          'unknownField',
          `Unknown field "${key}" is not part of OQL.`,
          {
            queryPath: key,
          }
        )
      );
    }
  }

  const select =
    raw.filesOnly === true
      ? Array.isArray(raw.select)
        ? raw.select
        : ['path', 'next.fetch']
      : raw.select;
  const view: OqlQuery['view'] =
    raw.filesOnly === true ? 'discovery' : (raw.view ?? 'paginated');

  const from = normalizeSource(raw, target as OqlQuery['target']);
  const scope = normalizeScope(raw, from);
  const where = normalizeWhere(raw, target as OqlQuery['target']);
  const materialize = normalizeMaterialize(
    raw,
    from,
    where,
    target as OqlQuery['target']
  );
  const fetch = normalizeFetch(raw);
  const params = normalizeParams(raw, target as OqlQuery['target']);

  const canonical: OqlQuery = {
    schema: 'oql',
    ...(raw.id ? { id: raw.id } : {}),
    target: target as OqlQuery['target'],
    ...(from ? { from } : {}),
    ...(params ? { params } : {}),
    ...(scope ? { scope } : {}),
    ...(where ? { where } : {}),
    ...(materialize ? { materialize } : {}),
    ...(fetch ? { fetch } : {}),
    ...(select ? { select } : {}),
    view,
    ...(raw.controls ? { controls: raw.controls } : {}),
    ...(raw.limit !== undefined ? { limit: raw.limit } : {}),
    ...(raw.page !== undefined ? { page: raw.page } : {}),
    ...(raw.itemsPerPage !== undefined
      ? { itemsPerPage: raw.itemsPerPage }
      : {}),
    ...(raw.explain !== undefined ? { explain: raw.explain } : {}),
  };

  // `code` requires a real predicate.
  if (canonical.target === 'code' && !canonical.where) {
    fail(
      diagnostic(
        'invalidQuery',
        'target:"code" requires a `where` predicate (text/regex/structural). `where` omission is not a wildcard.',
        { queryPath: 'where' }
      )
    );
  }

  // `content`/`structure` do not evaluate `where` (the execution layer would
  // silently drop it). Reject rather than drop — no predicate may disappear.
  if (
    (canonical.target === 'content' || canonical.target === 'structure') &&
    canonical.where
  ) {
    fail(
      diagnostic(
        'invalidQuery',
        `target:"${canonical.target}" does not use \`where\`. Use fetch.content.match for content anchors, or target:"code"/"files" for predicates.`,
        { queryPath: 'where' }
      )
    );
  }

  // `content`/`structure` reads of a specific tree require a concrete GitHub
  // repository (`owner/name`). Provider-wide or owner-only GitHub sources are
  // valid only for provider-search targets (code/repositories), not for fetching
  // a specific file or directory tree (contract §source-and-scope).
  if (
    (canonical.target === 'content' || canonical.target === 'structure') &&
    canonical.from?.kind === 'github' &&
    !(canonical.from.repo && canonical.from.repo.includes('/'))
  ) {
    fail(
      diagnostic(
        'invalidQuery',
        `target:"${canonical.target}" over GitHub requires a concrete repository ("owner/name"); a provider-wide or owner-only source cannot read a specific tree.`,
        {
          queryPath: 'from',
          repair: {
            message:
              'Set from:{kind:"github",repo:"owner/name"} (and scope.path for a subtree).',
          },
        }
      )
    );
  }

  // target:"materialize" is a clone checkpoint, not a search: it takes no
  // `where`, and needs a materializable source (GitHub repo, or an already
  // materialized path to echo).
  if (canonical.target === 'materialize') {
    if (canonical.where) {
      fail(
        diagnostic(
          'invalidQuery',
          'target:"materialize" does not use `where`; it clones/caches a corpus and returns a stable local checkpoint. Run a search against the returned localPath instead.',
          { queryPath: 'where' }
        )
      );
    }
    if (
      canonical.from?.kind !== 'github' &&
      canonical.from?.kind !== 'materialized'
    ) {
      fail(
        diagnostic(
          'invalidQuery',
          'target:"materialize" needs from:{kind:"github",repo:"owner/name"} (and scope.path to bound the subtree) or an already-materialized `from`.',
          {
            queryPath: 'from',
            repair: {
              message:
                'Set from:{kind:"github",repo:"owner/name"} with scope.path.',
            },
          }
        )
      );
    }
  }

  // Typed params check: catch type mistakes on known params fields early
  // (the backing tool remains the exhaustive validator for the rest).
  if (canonical.params !== undefined) {
    const paramsError = validateTargetParams(
      canonical.target,
      canonical.params
    );
    if (paramsError) {
      fail(diagnostic('invalidQuery', paramsError, { queryPath: 'params' }));
    }
  }

  // Validate the final canonical object against the strict schema.
  const check = OqlQuerySchema.safeParse(canonical);
  if (!check.success) {
    fail(diagnostic('invalidQuery', formatZodError(check.error)));
  }
  return check.data as OqlQuery;
}

/**
 * Infer the result target from sugar when not explicit:
 *  - any match sugar / canonical `where` -> "code"
 *  - fetch.content -> "content"
 *  - fetch.tree -> "structure"
 */
function inferTarget(raw: OqlInputQuery): OqlQuery['target'] | undefined {
  const hasMatch =
    raw.where !== undefined ||
    typeof raw.text === 'string' ||
    typeof raw.regex === 'string' ||
    typeof raw.pattern === 'string' ||
    raw.rule !== undefined ||
    Array.isArray(raw.and) ||
    Array.isArray(raw.or) ||
    Array.isArray(raw.xor) ||
    Array.isArray(raw.noneOf) ||
    Array.isArray(raw.oneOf);
  if (hasMatch) return 'code';
  if (raw.fetch?.content) return 'content';
  if (raw.fetch?.tree) return 'structure';
  return undefined;
}

/* ------------------------------- params ---------------------------------- */

const GRAPH_LSP_PROOF_TERMS = [
  'relationship',
  'relationships',
  'reference',
  'references',
  'who uses',
  'used by',
  'usage',
  'caller',
  'callers',
  'callee',
  'callees',
  'call hierarchy',
  'blast radius',
  'safe to delete',
  'what breaks',
  'delete',
  'dead code',
  'unused export',
  'unused symbol',
  'retained by',
];

function normalizeParams(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): Record<string, unknown> | undefined {
  const params = raw.params
    ? { ...(raw.params as Record<string, unknown>) }
    : undefined;
  if (target !== 'graph' || !params) return params;
  if (!shouldDefaultGraphLspProof(params)) return params;
  return {
    ...params,
    proof: 'lsp',
    proofLimit:
      typeof params.proofLimit === 'number' && params.proofLimit > 0
        ? params.proofLimit
        : 5,
  };
}

function shouldDefaultGraphLspProof(params: Record<string, unknown>): boolean {
  if (params.proof !== undefined || params.mode === 'plan') return false;
  if (params.mode === 'prove') return false;
  if (params.relation !== undefined || params.direction !== undefined)
    return true;
  const goal = typeof params.goal === 'string' ? params.goal.toLowerCase() : '';
  return GRAPH_LSP_PROOF_TERMS.some(term => goal.includes(term));
}

/* ------------------------------ source ---------------------------------- */

function normalizeSource(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): QuerySource | undefined {
  const explicitFrom = raw.from;
  const hasRepoSugar =
    typeof raw.repo === 'string' || typeof raw.owner === 'string';
  const topPath = raw.path;

  if (explicitFrom) {
    if (hasRepoSugar) {
      fail(
        diagnostic(
          'ambiguousSugar',
          'Provide either `from` or top-level repo/owner sugar, not both.',
          { queryPath: 'from' }
        )
      );
    }
    return normalizeGithubIdentity(explicitFrom);
  }

  if (hasRepoSugar) {
    const owner = typeof raw.owner === 'string' ? raw.owner : undefined;
    let repo = typeof raw.repo === 'string' ? raw.repo : undefined;
    if (owner && repo && !repo.includes('/')) {
      repo = `${owner}/${repo}`;
    }
    const src: QuerySource = { kind: 'github' };
    if (repo) src.repo = repo;
    if (owner && !repo) src.owner = owner;
    if (typeof raw.ref === 'string') src.ref = raw.ref;
    return src;
  }

  // no repo, no explicit from
  if (typeof topPath === 'string') {
    return { kind: 'local', path: topPath };
  }
  if (Array.isArray(topPath) && typeof topPath[0] === 'string') {
    // OQL accepts one canonical corpus root. If legacy callers pass multiple
    // roots, normalization keeps the first and ignores the rest.
    return { kind: 'local', path: topPath[0] };
  }

  // packages discovery defaults to the npm registry corpus.
  if (target === 'packages') return { kind: 'npm' };
  // repositories discovery may be provider-wide (no concrete repo).
  if (target === 'repositories') return { kind: 'github' };
  // other corpus-optional targets simply have no corpus.
  if (CORPUS_OPTIONAL_TARGETS.includes(target)) return undefined;

  fail(
    diagnostic(
      'invalidQuery',
      'A corpus is required: provide `from`, a `repo`, or a local `path`.',
      { queryPath: 'from' }
    )
  );
}

function normalizeGithubIdentity(from: QuerySource): QuerySource {
  if (from.kind !== 'github') return from;
  const owner = from.owner;
  let repo = from.repo;
  if (owner && repo && !repo.includes('/')) {
    repo = `${owner}/${repo}`;
    const next: QuerySource = { kind: 'github', repo };
    if (from.ref) next.ref = from.ref;
    return next;
  }
  return from;
}

/* ------------------------------- scope ---------------------------------- */

function normalizeScope(
  raw: OqlInputQuery,
  from: QuerySource | undefined
): QueryScope | undefined {
  const scope: QueryScope = { ...(raw.scope ?? {}) };

  // path sugar resolution
  const topPath = raw.path;
  const usesTopPathAsSource =
    from?.kind === 'local' &&
    !raw.from &&
    typeof raw.repo !== 'string' &&
    typeof raw.owner !== 'string';

  if (topPath !== undefined && !usesTopPathAsSource) {
    if (raw.scope && raw.scope.path !== undefined) {
      fail(
        diagnostic(
          'ambiguousSugar',
          'Both top-level `path` and `scope.path` provided; the path intent is ambiguous.',
          { queryPath: 'path' }
        )
      );
    }
    scope.path = topPath;
  }

  return Object.keys(scope).length > 0 ? scope : undefined;
}

/* ------------------------------- where ---------------------------------- */

function normalizeWhere(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): Predicate | undefined {
  const sugarPredicate = buildSugarPredicate(raw);

  if (raw.where && sugarPredicate) {
    fail(
      diagnostic(
        'ambiguousSugar',
        'Provide either a canonical `where` predicate or top-level match sugar, not both.',
        { queryPath: 'where' }
      )
    );
  }

  let predicate = raw.where ?? sugarPredicate;

  // `filesWithoutMatch` sugar becomes target:"files" + not(predicate). The
  // target flip is handled above; here we only wrap the predicate.
  if (raw.filesWithoutMatch && predicate) {
    predicate = { kind: 'not', predicate };
  }

  // invert sugar wraps the whole predicate
  if (raw.invert === true && predicate) {
    predicate = { kind: 'not', predicate };
  }

  if (!predicate) {
    if (target === 'code') return undefined; // caller raises invalidQuery
    return undefined;
  }

  validatePredicate(predicate, 'where');

  // Validate via schema (catches malformed shapes/unknown keys inside where).
  const check = PredicateSchema.safeParse(predicate);
  if (!check.success) {
    fail(
      diagnostic('invalidQuery', formatZodError(check.error), {
        queryPath: 'where',
      })
    );
  }
  return check.data as Predicate;
}

/** Generated predicate-node count, for the maxBooleanExpansion budget. */
function countNodes(p: Predicate): number {
  if (p.kind === 'all' || p.kind === 'any') {
    return 1 + p.of.reduce((n, c) => n + countNodes(c), 0);
  }
  if (p.kind === 'not') return 1 + countNodes(p.predicate);
  return 1;
}

/** Default mirrors DEFAULTS.maxBooleanExpansion (kept local to avoid a cycle). */
const DEFAULT_MAX_BOOLEAN_EXPANSION = 64;

function booleanExpansionBudget(raw: OqlInputQuery): number {
  const b = (
    raw.controls as { budget?: { maxBooleanExpansion?: number } } | undefined
  )?.budget?.maxBooleanExpansion;
  return typeof b === 'number' && b > 0 ? b : DEFAULT_MAX_BOOLEAN_EXPANSION;
}

/**
 * Build a predicate from top-level match sugar. Boolean sugar (and/or/xor/
 * noneOf/oneOf) is normalized to canonical all/any/not here. Expansions that
 * generate more nodes than `controls.budget.maxBooleanExpansion` fail with
 * `budgetExhausted` rather than ballooning the plan.
 */
function buildSugarPredicate(raw: OqlInputQuery): Predicate | undefined {
  const checkBudget = (p: Predicate): Predicate => {
    const nodes = countNodes(p);
    const budget = booleanExpansionBudget(raw);
    if (nodes > budget) {
      fail(
        diagnostic(
          'budgetExhausted',
          `Boolean sugar expanded to ${nodes} predicate nodes, over controls.budget.maxBooleanExpansion (${budget}). Narrow the query or raise the budget.`,
          { queryPath: 'where' }
        )
      );
    }
    return p;
  };
  // boolean sugar
  if (Array.isArray(raw.and)) {
    return { kind: 'all', of: raw.and.map(coercePredicate) };
  }
  if (Array.isArray(raw.or)) {
    return { kind: 'any', of: raw.or.map(coercePredicate) };
  }
  if (Array.isArray(raw.noneOf)) {
    return {
      kind: 'not',
      predicate: { kind: 'any', of: raw.noneOf.map(coercePredicate) },
    };
  }
  if (Array.isArray(raw.xor)) {
    if (raw.xor.length !== 2) {
      fail(
        diagnostic(
          'invalidQuery',
          'xor is binary; use oneOf for multi-way exclusive matching.',
          { queryPath: 'xor' }
        )
      );
    }
    const a = coercePredicate(raw.xor[0]);
    const b = coercePredicate(raw.xor[1]);
    return checkBudget({
      kind: 'any',
      of: [
        { kind: 'all', of: [a, { kind: 'not', predicate: b }] },
        { kind: 'all', of: [{ kind: 'not', predicate: a }, b] },
      ],
    });
  }
  if (Array.isArray(raw.oneOf)) {
    return checkBudget(expandOneOf(raw.oneOf.map(coercePredicate)));
  }

  // structural sugar
  if (typeof raw.pattern === 'string' || raw.rule !== undefined) {
    if (typeof raw.pattern === 'string' && raw.rule !== undefined) {
      fail(
        diagnostic(
          'invalidQuery',
          'A structural predicate uses exactly one of `pattern` or `rule`.',
          { queryPath: 'pattern' }
        )
      );
    }
    if (typeof raw.lang !== 'string') {
      fail(
        diagnostic('invalidQuery', 'Structural sugar requires `lang`.', {
          queryPath: 'lang',
        })
      );
    }
    return {
      kind: 'structural',
      lang: raw.lang,
      ...(typeof raw.pattern === 'string' ? { pattern: raw.pattern } : {}),
      ...(raw.rule !== undefined
        ? { rule: raw.rule as StructuralRuleInput }
        : {}),
    };
  }

  if (typeof raw.text === 'string') {
    return { kind: 'text', value: raw.text };
  }
  if (typeof raw.regex === 'string') {
    return { kind: 'regex', value: raw.regex };
  }
  return undefined;
}

function coercePredicate(p: unknown): Predicate {
  if (p && typeof p === 'object' && 'kind' in (p as Record<string, unknown>)) {
    return p as Predicate;
  }
  fail(
    diagnostic(
      'invalidQuery',
      'Boolean sugar children must be predicate objects with a `kind`.',
      { queryPath: 'where' }
    )
  );
}

function expandOneOf(predicates: Predicate[]): Predicate {
  // exactly-one expansion: any( all(Pi, not(Pj!=i)) )
  const branches: Predicate[] = predicates.map((p, i) => {
    const negatives = predicates
      .filter((_, j) => j !== i)
      .map<Predicate>(q => ({ kind: 'not', predicate: q }));
    return { kind: 'all', of: [p, ...negatives] };
  });
  return { kind: 'any', of: branches };
}

function validatePredicate(p: Predicate, path: string): void {
  switch (p.kind) {
    case 'all':
    case 'any':
      if (!Array.isArray(p.of) || p.of.length === 0) {
        fail(
          diagnostic('invalidQuery', `Empty \`${p.kind}.of\` is invalid.`, {
            queryPath: path,
          })
        );
      }
      p.of.forEach((c, i) => validatePredicate(c, `${path}.of[${i}]`));
      break;
    case 'not':
      if (!p.predicate) {
        fail(
          diagnostic('invalidQuery', '`not` must contain exactly one child.', {
            queryPath: path,
          })
        );
      }
      validatePredicate(p.predicate, `${path}.predicate`);
      break;
    case 'structural':
      if ((typeof p.pattern === 'string') === (p.rule !== undefined)) {
        fail(
          diagnostic(
            'invalidQuery',
            'A structural predicate uses exactly one of `pattern` or `rule`.',
            { queryPath: path }
          )
        );
      }
      break;
    case 'field':
      validateFieldPredicate(p, path);
      break;
    default:
      break;
  }
}

function validateFieldPredicate(
  p: { field: string; op: string; value?: unknown },
  path: string
): void {
  if (p.op === 'exists') {
    if (p.value !== undefined) {
      fail(
        diagnostic('fieldTypeMismatch', '`exists` takes no value.', {
          queryPath: path,
        })
      );
    }
    return;
  }
  if (p.value === undefined) {
    fail(
      diagnostic('fieldTypeMismatch', `Operator "${p.op}" requires a value.`, {
        queryPath: path,
      })
    );
  }
  if (p.op === 'in' && (!Array.isArray(p.value) || p.value.length === 0)) {
    fail(
      diagnostic(
        'fieldTypeMismatch',
        '`in` requires a non-empty array of values.',
        { queryPath: path }
      )
    );
  }
}

/* ---------------------------- materialize ------------------------------- */

function isLocalOnlyPredicate(p: Predicate | undefined): boolean {
  if (!p) return false;
  switch (p.kind) {
    case 'structural':
      return true;
    case 'regex':
      return p.dialect === 'pcre2';
    case 'all':
    case 'any':
      // A multi-leaf boolean cannot be a single provider call, so it needs a
      // local/materialized corpus (set-algebra over per-leaf results).
      return true;
    case 'not':
      return isLocalOnlyPredicate(p.predicate);
    default:
      return false;
  }
}

function normalizeMaterialize(
  raw: OqlInputQuery,
  from: QuerySource | undefined,
  where: Predicate | undefined,
  target: OqlQuery['target']
): MaterializePolicy | undefined {
  let policy: MaterializePolicy | undefined;
  if (typeof raw.materialize === 'string') {
    policy = { mode: raw.materialize };
  } else if (raw.materialize && typeof raw.materialize === 'object') {
    policy = raw.materialize as MaterializePolicy;
  }

  // target:"materialize" IS a clone op: it must materialize. Force mode away
  // from "never" and default to a bounded subtree so the planner's bounded-scope
  // safety check applies (an unbounded subtree without scope.path is refused).
  if (target === 'materialize' && from?.kind === 'github') {
    if (!policy) return { mode: 'required', strategy: 'subtree' };
    return {
      ...policy,
      mode: policy.mode === 'never' ? 'required' : policy.mode,
      strategy: policy.strategy ?? 'subtree',
    };
  }

  // Remote semantics has no provider-only lane: the adapter sparsely
  // materializes the requested file/repo, then runs LSP locally. Normalize that
  // internal route explicitly so `--explain` never says provider-only while
  // listing ghCloneRepo + lspGetSemantics backend calls.
  if (target === 'semantics' && from?.kind === 'github') {
    return {
      ...(policy ?? {}),
      mode: 'required',
      strategy: 'file',
    };
  }

  if (from?.kind !== 'github') {
    // local/materialized/npm/no-corpus sources don't need a materialize policy
    return policy;
  }

  if (policy) return policy;

  // GitHub source, no explicit policy: a local-only predicate (structural /
  // PCRE2) cannot be proven by the provider, so default to bounded
  // remote-as-local. Provider-capable predicates default to provider-only.
  if (isLocalOnlyPredicate(where)) {
    return { mode: 'auto', strategy: 'subtree' };
  }
  return { mode: 'never' };
}

/* ------------------------------- fetch ---------------------------------- */

function normalizeFetch(raw: OqlInputQuery): OqlQuery['fetch'] | undefined {
  return raw.fetch ? { ...raw.fetch } : undefined;
}

/* ------------------------------ helpers --------------------------------- */

function formatZodError(error: unknown): string {
  const e = error as { issues?: Array<{ path: unknown[]; message: string }> };
  if (e && Array.isArray(e.issues)) {
    return e.issues
      .map(i => `${i.path.join('.') || '(root)'}: ${i.message}`)
      .join('; ');
  }
  return 'Invalid OQL query.';
}

export { asArray };
