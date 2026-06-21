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
import { classifyDiffLane, diffLaneBackend } from './diffLanes.js';
import { checkOutputFeatures } from './features.js';
import { diagnostic } from './diagnostics.js';
import { DEFAULTS, appliedDefaults } from './defaults.js';
import type {
  LeafPredicate,
  MaterializePolicy,
  OqlBackendCall,
  OqlDiagnostic,
  OqlExplainPlan,
  OqlPlanNode,
  OqlQueryV1,
  PlanRoute,
  Predicate,
  QuerySource,
} from './types.js';

interface WalkResult {
  nodes: OqlPlanNode[];
  diagnostics: OqlDiagnostic[];
  backendCalls: OqlBackendCall[];
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
  query: OqlQueryV1,
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
    const route = combineBooleanRoute(predicate.kind, childRoutes);
    out.nodes.push({
      predicateId: id,
      path,
      route,
      reason: `${predicate.kind} over ${childRoutes.length} children`,
    });
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
      source: query.from,
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

function operationFor(target: OqlQueryV1['target']): string {
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

export function planQuery(
  query: OqlQueryV1,
  rawInput: unknown
): PlanQueryResult {
  const out: WalkResult = { nodes: [], diagnostics: [], backendCalls: [] };
  const materialize = query.materialize;
  const source: QuerySource = query.from ?? { kind: 'github' };
  const ctx: CapabilityContext = {
    sourceKind: source.kind === 'npm' ? 'github' : source.kind,
    target: query.target,
    materialize,
  };

  // Predicate routing
  if (query.where) {
    walkPredicate(query, query.where, 'where', ctx, out);
  } else if (query.target === 'diff') {
    // target:"diff" routes by params shape, not target alone. The lane
    // discriminant is shared with the adapter (diffLanes.ts) so the dry-run
    // plan can never contradict execution on backend name or executability.
    const lane = classifyDiffLane(query.params);
    const backend = diffLaneBackend(lane);
    if (backend) {
      out.backendCalls.push({
        backend,
        source,
        operation: operationFor('diff'),
        exact: true,
      });
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
    // targetless families (content/structure/files + V2 research targets):
    // a single fetch/list/inspect backend call.
    out.backendCalls.push({
      backend: backendForTargetless(query),
      source,
      operation: operationFor(query.target),
      exact: query.target !== 'research',
    });
  }

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

function backendForTargetless(query: OqlQueryV1): string {
  const local = query.from?.kind !== 'github';
  switch (query.target) {
    case 'content':
      return local ? 'localGetFileContent' : 'ghGetFileContent';
    case 'structure':
      return local ? 'localViewStructure' : 'ghViewRepoStructure';
    case 'files':
      return 'localFindFiles';
    case 'semantics':
      return 'lspGetSemantics';
    case 'repositories':
      return 'ghSearchRepos';
    case 'packages':
      return 'npmSearch';
    case 'pullRequests':
    case 'commits':
      return 'ghHistoryResearch';
    case 'artifacts':
      return 'localBinaryInspect';
    case 'research':
      return 'smartOqlResearch';
    // 'diff' is owned by the lane-aware branch in planQuery (diffLanes.ts) and
    // never reaches here.
    case 'materialize':
      return 'ghCloneRepo';
    default:
      return local ? 'localSearchCode' : 'ghSearchCode';
  }
}

function decideMaterialization(
  query: OqlQueryV1,
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
