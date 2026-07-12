/**
 * Predicate-tree walking + transformer routing for the OQL planner.
 *
 * Responsible for:
 *  - walking the predicate tree and routing every leaf/boolean node
 *    (invariant: pushed + residual + routed + unsupported == all predicate
 *    nodes);
 *  - recording backend calls and transformer traces as routing decisions are
 *    made, including the GitHub materialization fallback for multi-leaf
 *    booleans and unrouted `files`/`code` targets;
 *  - picking the transformer registry entry for a given query/source pair.
 */
import { routeLeafPredicate, type CapabilityContext } from '../capabilities.js';
import { diagnostic } from '../diagnostics.js';
import {
  backendCallsForTransformer,
  findTransformerById,
  findTransformerEntry,
  findTransformerForQuery,
  transformerTrace,
} from '../transformers/registry.js';
import type { TransformerRegistryEntry } from '../transformers/contract.js';
import type {
  LeafPredicate,
  OqlBackendCall,
  OqlDiagnostic,
  OqlExplainPlan,
  OqlPlanNode,
  OqlQuery,
  PlanRoute,
  Predicate,
  QuerySource,
} from '../types.js';

export interface WalkResult {
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
export function walkPredicate(
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

export function addBackendCall(
  calls: OqlBackendCall[],
  call: OqlBackendCall
): void {
  const exists = calls.some(
    c =>
      c.backend === call.backend &&
      c.operation === call.operation &&
      c.exact === call.exact
  );
  if (!exists) calls.push(call);
}

export function addTransformerTrace(
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

export function addTransformerBackendCalls(
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

export function addGithubFilesMaterializationCalls(
  out: WalkResult,
  source: QuerySource
): boolean {
  return addGithubMaterializationCalls(out, source, 'files');
}

export function addGithubMaterializationCalls(
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

export function containsStructuralPredicate(
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

export function localTransformerForRoutedQuery(
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

export function transformerForQuery(
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
