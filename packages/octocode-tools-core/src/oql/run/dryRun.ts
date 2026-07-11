/**
 * `--dry-run` / non-executable-plan envelopes: report the plan and diagnostics
 * without executing, plus repair/upgrade continuations (materialize, graph
 * proof) so the caller isn't left at a dead end.
 */
import { diagnostic } from '../diagnostics.js';
import { unsupportedEnvelope } from '../envelope.js';
import type { PlanQueryResult } from '../planner.js';
import type {
  OqlContinuation,
  OqlDiagnostic,
  OqlQuery,
  OqlResultEnvelope,
} from '../types.js';

/**
 * `target:"files"` over a GitHub source with no `where` cannot be enumerated by
 * the provider, so the plan is non-executable (audit #6). Instead of a dead end,
 * hand the agent a runnable next.materialize query — clone a bounded corpus and
 * list files from the materialized checkpoint — mirroring the providerUnindexed
 * recovery path.
 */
export function blockedMaterializeContinuation(
  query?: OqlQuery
): OqlResultEnvelope['next'] | undefined {
  if (
    !query ||
    query.from?.kind !== 'github' ||
    query.target !== 'files' ||
    query.where
  ) {
    return undefined;
  }
  return {
    'next.materialize': {
      query: {
        schema: 'oql',
        target: 'materialize',
        from: query.from,
        ...(query.scope ? { scope: query.scope } : {}),
        materialize: { mode: 'required' },
      },
      why: 'target:"files" over GitHub needs a local corpus to enumerate; clone a bounded path (add scope.path to narrow), then list files from the materialized checkpoint.',
      confidence: 'heuristic',
    },
  };
}

export function unsupportedEnvelopeFromPlan(
  planned: PlanQueryResult,
  plan: OqlResultEnvelope['plan'],
  queryId?: string,
  queryIndex?: number,
  dryRun?: boolean,
  query?: OqlQuery
): OqlResultEnvelope {
  const dryRunGuidance =
    dryRun && query ? dryRunResearchGraphGuidance(query) : [];
  const dryRunNext =
    dryRun && query ? dryRunResearchGraphNext(query) : undefined;

  if (!planned.executable) {
    // In dry-run mode, distinguish repairable blocks (e.g. missing scope.path
    // for materialization) from structural capability gaps (UNSUPPORTED route
    // nodes). Repairable queries have a valid plan with executable routing
    // decisions — they just need a constraint fix. Show 'partial' so the plan
    // and diagnostics are the primary output, not 'unsupported'.
    const hasUnsupportedRoute = planned.plan.nodes.some(
      n => n.route === 'UNSUPPORTED'
    );
    if (dryRun && !hasUnsupportedRoute) {
      return {
        ...(queryId ? { queryId } : {}),
        ...(queryIndex !== undefined ? { queryIndex } : {}),
        results: [],
        diagnostics: [...planned.plan.diagnostics, ...dryRunGuidance],
        provenance: [],
        evidence: { answerReady: false, complete: false, kind: 'partial' },
        ...(plan ? { plan } : {}),
        ...(dryRunNext ? { next: dryRunNext } : {}),
      };
    }
    return unsupportedEnvelope(
      planned.plan.diagnostics,
      plan,
      queryId,
      queryIndex,
      blockedMaterializeContinuation(query)
    );
  }
  // dry run of an executable query: report plan, evidence partial (not executed)
  return {
    ...(queryId ? { queryId } : {}),
    ...(queryIndex !== undefined ? { queryIndex } : {}),
    results: [],
    diagnostics: [...planned.plan.diagnostics, ...dryRunGuidance],
    provenance: [],
    evidence: { answerReady: false, complete: false, kind: 'partial' },
    ...(plan ? { plan } : {}),
    ...(dryRunNext ? { next: dryRunNext } : {}),
  };
}

function dryRunResearchGraphGuidance(query: OqlQuery): OqlDiagnostic[] {
  if (query.target !== 'research' && query.target !== 'graph') return [];

  const message =
    query.target === 'research'
      ? 'Dry run only planned target:"research"; execute without --dry-run to get the summary plus paged candidate packets. Compact text shows packet subject IDs and next.graph; follow next.graph to upgrade the current page to bounded LSP proof.'
      : 'Dry run only planned target:"graph"; execute without --dry-run to get reachability packets. Use params:{mode:"prove",proof:"lsp"} or follow next.graph to run bounded LSP proof for the current page.';

  return [
    diagnostic('partialResult', message, {
      backend: query.target === 'graph' ? 'smartOqlGraph' : 'smartOqlResearch',
      blocksAnswer: false,
      severity: 'info',
    }),
  ];
}

function dryRunResearchGraphNext(
  query: OqlQuery
): Record<string, OqlContinuation> | undefined {
  if (query.target !== 'research' && query.target !== 'graph') return undefined;
  const from = query.from;
  if (from?.kind !== 'local' && from?.kind !== 'materialized') return undefined;

  const params = query.params ?? {};
  const intent =
    typeof params.intent === 'string' && params.intent.length > 0
      ? params.intent
      : 'reachability';
  const proofLimit = Math.min(25, Math.max(1, query.itemsPerPage ?? 10));

  return {
    'next.graph': {
      query: {
        schema: 'oql',
        target: 'graph',
        from,
        params: {
          ...params,
          mode: 'prove',
          proof: 'lsp',
          intent,
          proofLimit,
        },
        ...(query.page ? { page: query.page } : {}),
        ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
      },
      why: 'Run the bounded LSP proof lane for this research/graph page.',
      confidence: 'exact',
    },
  };
}
