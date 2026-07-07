/**
 * OQL planner — converts a normalized canonical query into a deterministic
 * plan + explain output.
 *
 * Guarantees:
 *  - every predicate node is preserved and routed (invariant:
 *    pushed + residual + routed + unsupported == all predicate nodes);
 *  - stable predicate IDs are derived from node position (or a user `id`);
 *  - diagnostics are first-class; materialization decisions are explicit;
 *  - `--explain` output never changes execution semantics (plan truncation
 *    emits `planTruncated` only).
 */
import { routeLeafPredicate, type CapabilityContext } from './capabilities.js';
import { classifyDiffLane } from './diffLanes.js';
import { checkOutputFeatures } from './features.js';
import { diagnostic } from './diagnostics.js';
import { DEFAULTS, appliedDefaults } from './defaults.js';
import { toGithubCodeSearchToolQuery } from './transformers/github/code.js';
import {
  backendCallsForTransformer,
  findTransformerById,
  findTransformerEntry,
  findTransformerForQuery,
  transformerTrace,
} from './transformers/registry.js';
import type { TransformerRegistryEntry } from './transformers/contract.js';
import type {
  LeafPredicate,
  MaterializePolicy,
  OqlBackendCall,
  OqlDiagnostic,
  OqlExplainPlan,
  OqlPlanNode,
  OqlQuery,
  PlanRoute,
  Predicate,
  QuerySource,
} from './types.js';
import { SEARCH_SORTS_BY_TARGET } from './types.js';

interface WalkResult {
  nodes: OqlPlanNode[];
  diagnostics: OqlDiagnostic[];
  backendCalls: OqlBackendCall[];
  transformers: OqlExplainPlan['transformers'];
}

function predicateId(p: Predicate, path: string): string {
  return p.id ?? path;
}

/**
 * Walk the predicate tree, routing every leaf and recording a node per
 * predicate (boolean nodes included). Boolean nodes are routed by composing
 * child routes.
 */
function walkPredicate(
  query: OqlQuery,
  predicate: Predicate,
  path: string,
  ctx: CapabilityContext,
  out: WalkResult,
  inNegation = false
): PlanRoute {
  const id = predicateId(predicate, path);

  if (predicate.kind === 'all' || predicate.kind === 'any') {
    const childRoutes = predicate.of.map((c, i) =>
      walkPredicate(query, c, `${path}.of[${i}]`, ctx, out, inNegation)
    );
    let route = combineBooleanRoute(predicate.kind, childRoutes);
    let reason = `${predicate.kind} over ${childRoutes.length} children`;

    // A multi-leaf boolean is not a single provider call. Over a GitHub
    // `code`/`files` source it must materialize (clone -> local set-algebra) or
    // it is unsupported — so the plan matches execution (the boolean evaluators
    // run only on a local/materialized corpus).
    if (
      ctx.sourceKind === 'github' &&
      (ctx.target === 'code' || ctx.target === 'files') &&
      route !== 'UNSUPPORTED'
    ) {
      const canMat =
        ctx.materialize?.mode === 'auto' ||
        ctx.materialize?.mode === 'required';
      if (canMat) {
        route = 'ROUTE';
        reason +=
          ' (routed to materialization: GitHub cannot evaluate a multi-leaf boolean in one call)';
      } else {
        route = 'UNSUPPORTED';
        reason +=
          ' (GitHub cannot evaluate a multi-leaf boolean; materialize for local proof)';
        out.diagnostics.push(
          diagnostic(
            'requiresMaterialization',
            'A multi-leaf boolean over a GitHub code source needs bounded materialization (clone then local set-algebra).',
            {
              queryPath: path,
              repair: {
                message:
                  'Add materialize:{mode:"auto"} with scope.path, or run one query per branch.',
              },
            }
          )
        );
      }
    }

    out.nodes.push({ predicateId: id, path, route, reason });
    return route;
  }

  if (predicate.kind === 'not') {
    // Negation flips parity for descendants. The leaf router decides, per
    // source, whether the negated predicate is exact (local/materialized
    // complete universe), needs materialization (provider ROUTE), or is
    // unprovable (provider with materialize.mode:"never" -> UNSUPPORTED +
    // negativeUniverseRequired). Double negation collapses to positive.
    const childRoute = walkPredicate(
      query,
      predicate.predicate,
      `${path}.predicate`,
      ctx,
      out,
      !inNegation
    );
    out.nodes.push({
      predicateId: id,
      path,
      route: childRoute,
      reason: 'not requires a complete evaluation universe to be exact',
    });
    return childRoute;
  }

  // leaf
  const decision = routeLeafPredicate(
    ctx,
    predicate as LeafPredicate,
    inNegation
  );
  out.nodes.push({
    predicateId: id,
    path,
    route: decision.route,
    backend: decision.backend,
    reason: decision.reason,
  });
  if (decision.diagnostic) {
    out.diagnostics.push(
      diagnostic(decision.diagnostic.code, decision.diagnostic.message, {
        predicateId: id,
        queryPath: path,
        backend: decision.backend,
      })
    );
  }
  if (decision.route !== 'UNSUPPORTED') {
    addBackendCall(out.backendCalls, {
      backend: decision.backend,
      source: decision.route === 'ROUTE' ? undefined : query.from,
      operation: operationFor(query.target),
      exact: decision.exact,
    });
  }
  return decision.route;
}

function combineBooleanRoute(
  kind: 'all' | 'any',
  children: PlanRoute[]
): PlanRoute {
  if (children.includes('UNSUPPORTED')) {
    // `all` can still push the supported part; `any` needs full union coverage.
    if (kind === 'any') return 'UNSUPPORTED';
  }
  if (children.every(r => r === 'PUSHDOWN')) return 'PUSHDOWN';
  if (children.includes('ROUTE')) return 'ROUTE';
  if (children.includes('RESIDUAL')) return 'RESIDUAL';
  if (children.includes('UNSUPPORTED')) return 'RESIDUAL';
  return children[0] ?? 'PUSHDOWN';
}

function operationFor(target: OqlQuery['target']): string {
  switch (target) {
    case 'code':
      return 'searchCode';
    case 'content':
      return 'getContent';
    case 'structure':
      return 'viewStructure';
    case 'files':
      return 'findFiles';
    case 'semantics':
      return 'getSemantics';
    case 'repositories':
      return 'searchRepos';
    case 'packages':
      return 'searchPackages';
    case 'pullRequests':
      return 'searchPullRequests';
    case 'commits':
      return 'searchCommits';
    case 'artifacts':
      return 'inspectArtifact';
    case 'diff':
      return 'diff';
    case 'research':
      return 'runResearchFlow';
    case 'graph':
      return 'queryRelationshipGraph';
    case 'materialize':
      return 'materialize';
  }
}

function addBackendCall(calls: OqlBackendCall[], call: OqlBackendCall): void {
  const exists = calls.some(
    c =>
      c.backend === call.backend &&
      c.operation === call.operation &&
      c.exact === call.exact
  );
  if (!exists) calls.push(call);
}

function addTransformerTrace(
  out: WalkResult,
  transformer: TransformerRegistryEntry | undefined
): void {
  if (!transformer) return;
  const trace = transformerTrace(transformer);
  const exists = out.transformers?.some(t => t.id === trace.id);
  if (!exists) {
    out.transformers = [...(out.transformers ?? []), trace];
  }
}

function addTransformerBackendCalls(
  out: WalkResult,
  transformer: TransformerRegistryEntry | undefined,
  source?: QuerySource
): boolean {
  if (!transformer) return false;
  for (const call of backendCallsForTransformer(transformer, source)) {
    addBackendCall(out.backendCalls, call);
  }
  return true;
}

function addGithubFilesMaterializationCalls(
  out: WalkResult,
  source: QuerySource
): boolean {
  return addGithubMaterializationCalls(out, source, 'files');
}

function addGithubMaterializationCalls(
  out: WalkResult,
  source: QuerySource,
  target: OqlQuery['target'],
  localTransformer: TransformerRegistryEntry | undefined = findTransformerEntry(
    {
      sourceKind: 'materialized',
      target,
    }
  )
): boolean {
  const materializeTransformer = findTransformerEntry({
    sourceKind: 'github',
    target: 'materialize',
  });
  if (!materializeTransformer || !localTransformer) return false;

  out.transformers = (out.transformers ?? []).filter(
    trace => trace.id !== `github.${target}`
  );
  addTransformerTrace(out, materializeTransformer);
  addTransformerTrace(out, localTransformer);

  const localBackendKeys = new Set(
    localTransformer.backends.map(
      backend => `${backend.backend}:${backend.operation}`
    )
  );
  out.backendCalls = out.backendCalls.filter(
    call => !localBackendKeys.has(`${call.backend}:${call.operation}`)
  );

  addTransformerBackendCalls(out, materializeTransformer, source);
  addTransformerBackendCalls(out, localTransformer, undefined);
  return true;
}

function containsStructuralPredicate(
  predicate: Predicate | undefined
): boolean {
  if (!predicate) return false;
  if (predicate.kind === 'structural') return true;
  if (predicate.kind === 'all' || predicate.kind === 'any') {
    return predicate.of.some(containsStructuralPredicate);
  }
  if (predicate.kind === 'not') {
    return containsStructuralPredicate(predicate.predicate);
  }
  return false;
}

function localTransformerForRoutedQuery(
  query: OqlQuery
): TransformerRegistryEntry | undefined {
  if (query.target === 'code') {
    return findTransformerById(
      containsStructuralPredicate(query.where)
        ? 'local.code.structural'
        : 'local.code.textRegex'
    );
  }
  return findTransformerEntry({
    sourceKind: 'materialized',
    target: query.target,
  });
}

function transformerForQuery(
  query: OqlQuery,
  source: QuerySource
): TransformerRegistryEntry | undefined {
  if (
    query.target === 'code' &&
    (source.kind === 'local' || source.kind === 'materialized')
  ) {
    return (
      findTransformerById(
        containsStructuralPredicate(query.where)
          ? 'local.code.structural'
          : 'local.code.textRegex'
      ) ??
      findTransformerForQuery({
        source,
        target: query.target,
        params: query.params,
      })
    );
  }
  return findTransformerForQuery({
    source,
    target: query.target,
    params: query.params,
  });
}

function countPredicateNodes(p: Predicate | undefined): number {
  if (!p) return 0;
  if (p.kind === 'all' || p.kind === 'any') {
    return 1 + p.of.reduce((n, c) => n + countPredicateNodes(c), 0);
  }
  if (p.kind === 'not') {
    return 1 + countPredicateNodes(p.predicate);
  }
  return 1;
}

export interface PlanQueryResult {
  plan: OqlExplainPlan;
  /** True when no node is UNSUPPORTED and no blocking diagnostic exists. */
  executable: boolean;
}

export function planQuery(query: OqlQuery, rawInput: unknown): PlanQueryResult {
  const out: WalkResult = {
    nodes: [],
    diagnostics: [],
    backendCalls: [],
    transformers: [],
  };
  const materialize = query.materialize;
  const source: QuerySource = query.from ?? { kind: 'github' };
  const ctx: CapabilityContext = {
    sourceKind: source.kind === 'npm' ? 'github' : source.kind,
    target: query.target,
    materialize,
  };
  const transformer = transformerForQuery(query, source);
  addTransformerTrace(out, transformer);

  // Predicate routing
  if (query.where) {
    walkPredicate(query, query.where, 'where', ctx, out);
    if (
      source.kind === 'github' &&
      out.nodes.some(node => node.route === 'ROUTE')
    ) {
      addGithubMaterializationCalls(
        out,
        source,
        query.target,
        localTransformerForRoutedQuery(query)
      );
    }
  } else if (query.target === 'diff') {
    // target:"diff" routes by params shape, not target alone. The lane
    // discriminant is shared with the adapter (diffLanes.ts) so the dry-run
    // plan can never contradict execution on backend name or executability.
    const lane = classifyDiffLane(query.params);
    if (lane.kind === 'prPatch' || lane.kind === 'directFile') {
      if (!addTransformerBackendCalls(out, transformer, source)) {
        out.diagnostics.push(
          diagnostic(
            'unsupportedTarget',
            `No transformer registered for target:"diff" lane "${lane.kind}".`,
            {
              queryPath: 'target',
              backend: 'ghHistoryResearch',
              severity: 'error',
            }
          )
        );
      }
    } else {
      out.diagnostics.push(
        diagnostic(
          'invalidQuery',
          'target:"diff" needs either {prNumber} (PR patch diff) or {baseRef,headRef,path} (direct file diff between two refs).',
          {
            queryPath: 'params',
            backend: 'ghHistoryResearch',
            repair: {
              message:
                'Add params.prNumber for a PR patch, or params.baseRef + params.headRef + params.path for a direct file diff.',
            },
          }
        )
      );
    }
  } else {
    // Targetless families use the transformer registry as the single backend
    // contract, keeping explain/dry-run aligned with adapter provenance.
    const githubFilesNeedsMaterialization =
      source.kind === 'github' && query.target === 'files' && !query.where;
    const canMaterialize =
      materialize?.mode === 'auto' || materialize?.mode === 'required';
    const added = githubFilesNeedsMaterialization
      ? canMaterialize && addGithubFilesMaterializationCalls(out, source)
      : addTransformerBackendCalls(out, transformer, source);
    if (!added && (!githubFilesNeedsMaterialization || canMaterialize)) {
      out.diagnostics.push(
        diagnostic(
          'unsupportedTarget',
          githubFilesNeedsMaterialization
            ? 'No transformer chain registered for target:"files" GitHub materialization.'
            : `No transformer registered for target:"${query.target}" from ${source.kind}.`,
          {
            queryPath: 'target',
            severity: 'error',
          }
        )
      );
    }
  }

  // `files` over a GitHub source with no `where` has no leaf to route through
  // the capability layer, but still cannot enumerate the file universe from the
  // provider. Enforce the same materialization requirement so the plan matches
  // executeGithub's files lane.
  if (source.kind === 'github' && query.target === 'files' && !query.where) {
    const canMat =
      materialize?.mode === 'auto' || materialize?.mode === 'required';
    if (!canMat) {
      out.diagnostics.push(
        diagnostic(
          'requiresMaterialization',
          'target:"files" over a GitHub source needs bounded materialization to enumerate files (set materialize.mode "auto"/"required" with scope.path), or use a local source.',
          {
            queryPath: 'target',
            backend: 'localFindFiles',
            // error: there is no provider lane to list the whole file set, and
            // no predicate node to carry an UNSUPPORTED route — block execution.
            severity: 'error',
            repair: {
              message:
                'Add materialize:{mode:"auto"} with scope.path, or use a local `from`.',
            },
          }
        )
      );
    }
  }

  out.diagnostics.push(...adapterValidationDiagnostics(query, out.nodes));
  out.diagnostics.push(...sortApplicabilityDiagnostics(query));

  // Output-feature capability check (content view / select projections). Emits
  // non-blocking diagnostics so a requested-but-unbackable feature is explicit,
  // never silently degraded.
  out.diagnostics.push(...checkOutputFeatures(query));

  // Materialization decision + safety checks
  const materializeDecision = decideMaterialization(query, out.diagnostics);

  // Budget / plan-node caps
  const maxPlanNodes =
    query.controls?.budget?.maxPlanNodes ?? DEFAULTS.maxPlanNodes;
  let truncated = false;
  let nodes = out.nodes;
  if (nodes.length > maxPlanNodes) {
    truncated = true;
    nodes = nodes.slice(0, maxPlanNodes);
    out.diagnostics.push(
      diagnostic(
        'planTruncated',
        `Explain plan truncated to ${maxPlanNodes} nodes; execution semantics are unchanged.`
      )
    );
  }

  const totalPredicateNodes = countPredicateNodes(query.where);
  // Invariant assertion (defensive; not user-facing). Boolean+leaf nodes are
  // all recorded, so out.nodes.length === totalPredicateNodes.
  if (!truncated && out.nodes.length !== totalPredicateNodes) {
    out.diagnostics.push(
      diagnostic(
        'invalidQuery',
        `Planner invariant violated: ${out.nodes.length} routed vs ${totalPredicateNodes} predicate nodes.`
      )
    );
  }

  const plan: OqlExplainPlan = {
    input: rawInput,
    normalized: query,
    defaults: appliedDefaults(query),
    nodes,
    backendCalls: out.backendCalls,
    ...(out.transformers?.length ? { transformers: out.transformers } : {}),
    ...(materializeDecision ? { materialization: materializeDecision } : {}),
    budgets: query.controls?.budget,
    ...(truncated ? { truncated } : {}),
    diagnostics: out.diagnostics,
  };

  const executable =
    !out.nodes.some(n => n.route === 'UNSUPPORTED') &&
    !out.diagnostics.some(
      d => d.severity === 'error' && d.code !== 'planTruncated'
    );

  return { plan, executable };
}

/** Collapse not(not(p)) → p recursively so double-negation never blocks validation. */
function collapseDoubleNegation(where: Predicate): Predicate {
  if (where.kind === 'not' && where.predicate.kind === 'not') {
    return collapseDoubleNegation(where.predicate.predicate);
  }
  return where;
}

/**
 * A `controls.search.sort` value outside the target lane's executable set is
 * IGNORED by the backend (files: localFindFiles sortBy; code: search ranking).
 * Warn instead of silently returning default ordering — non-blocking because
 * ordering never changes WHICH rows exist, only their sequence.
 */
function sortApplicabilityDiagnostics(query: OqlQuery): OqlDiagnostic[] {
  const sort = query.controls?.search?.sort;
  if (!sort) return [];
  const lane =
    query.target === 'code' || query.target === 'files'
      ? SEARCH_SORTS_BY_TARGET[query.target]
      : undefined;
  if (lane) {
    if ((lane as readonly string[]).includes(sort)) return [];
    return [
      diagnostic(
        'lossyTransform',
        `controls.search.sort:"${sort}" has no ${query.target}-lane equivalent and is ignored (default ordering applies). Sorts for target:"${query.target}": ${lane.join('|')}.`,
        {
          queryPath: 'controls.search.sort',
          severity: 'warning',
          blocksAnswer: false,
        }
      ),
    ];
  }
  // Non-search targets (repositories/packages/pullRequests/commits/research/
  // graph) have no controls.search.sort lane at all — the value is silently
  // dropped because only the code/files backends read it. Warn, and for the
  // targets that DO have their own ordering, point at the params passthrough.
  const passthroughHint =
    query.target === 'repositories' || query.target === 'packages'
      ? ` Pass the sort through params.sort instead (validated by the target:"${query.target}" params).`
      : '';
  return [
    diagnostic(
      'lossyTransform',
      `controls.search.sort:"${sort}" is not applied for target:"${query.target}" — only target:"code" and target:"files" support controls.search.sort.${passthroughHint}`,
      {
        queryPath: 'controls.search.sort',
        severity: 'warning',
        blocksAnswer: false,
      }
    ),
  ];
}

function adapterValidationDiagnostics(
  query: OqlQuery,
  nodes: OqlPlanNode[]
): OqlDiagnostic[] {
  if (
    query.from?.kind !== 'github' ||
    (query.target !== 'code' && query.target !== 'files') ||
    !query.where ||
    query.materialize?.mode === 'required' ||
    nodes.some(node => node.route === 'ROUTE')
  ) {
    return [];
  }

  const normalizedWhere = collapseDoubleNegation(query.where);
  const transformed = toGithubCodeSearchToolQuery(
    { ...query, where: normalizedWhere },
    {
      ...(query.target === 'files' ? { defaultMatch: 'file' as const } : {}),
    }
  );
  return transformed.ok ? [] : transformed.diagnostics;
}

function decideMaterialization(
  query: OqlQuery,
  diagnostics: OqlDiagnostic[]
): (MaterializePolicy & { required: boolean; reason: string }) | undefined {
  const m = query.materialize;
  if (!m || query.from?.kind !== 'github') return undefined;

  if (m.mode === 'never') {
    return {
      ...m,
      required: false,
      reason: 'provider-only execution (materialize.mode:"never")',
    };
  }

  // bounded scope is required for materialization
  const hasBoundedScope = Boolean(query.scope?.path) || m.strategy === 'file';
  const fullRepo = m.strategy === 'repo';

  if (fullRepo && !m.allowFullRepo) {
    diagnostics.push(
      diagnostic(
        'materializationNotAllowed',
        'strategy:"repo" requires allowFullRepo:true and a byte budget; repair to "subtree" with a concrete scope.path.',
        {
          queryPath: 'materialize',
          repair: {
            message: 'Use materialize.strategy:"subtree" with scope.path.',
          },
        }
      )
    );
  } else if (!fullRepo && !hasBoundedScope) {
    // BLOCKING: an unbounded subtree clone could pull the whole repo. The
    // contract requires materialization to be bounded and explicit, so refuse
    // to execute rather than warn-and-proceed.
    diagnostics.push(
      diagnostic(
        'materializationNotAllowed',
        'Bounded materialization needs scope.path (or strategy:"file"); refusing to clone an unbounded scope.',
        {
          queryPath: 'materialize',
          repair: {
            message: 'Add scope.path to bound the materialized subtree.',
          },
        }
      )
    );
  }

  return {
    ...m,
    strategy: m.strategy ?? 'subtree',
    required: m.mode === 'required',
    reason:
      m.mode === 'required'
        ? 'local-only proof required; must materialize before execution'
        : 'planner may materialize bounded source for local proof',
  };
}
