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
 *
 * This module is the orchestrator: predicate-tree walking + transformer
 * routing lives in ./planner/predicateRouting.js, and post-routing
 * diagnostics + materialization policy lives in ./planner/planDiagnostics.js.
 */
import type { CapabilityContext } from './capabilities.js';
import { classifyDiffLane } from './diffLanes.js';
import { checkOutputFeatures } from './features.js';
import { diagnostic } from './diagnostics.js';
import { DEFAULTS, appliedDefaults } from './defaults.js';
import { countPredicateNodes } from './predicateUtils.js';
import {
  addGithubFilesMaterializationCalls,
  addGithubMaterializationCalls,
  addTransformerBackendCalls,
  addTransformerTrace,
  localTransformerForRoutedQuery,
  transformerForQuery,
  walkPredicate,
  type WalkResult,
} from './planner/predicateRouting.js';
import {
  adapterValidationDiagnostics,
  decideMaterialization,
  sortApplicabilityDiagnostics,
} from './planner/planDiagnostics.js';
import type { OqlExplainPlan, OqlQuery, QuerySource } from './types.js';

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
