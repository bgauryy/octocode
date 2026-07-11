/**
 * Post-routing plan diagnostics + materialization policy for the OQL
 * planner.
 *
 * Responsible for:
 *  - validating that a GitHub `code`/`files` query which the planner believes
 *    is fully pushdown-able actually round-trips through the GitHub adapter
 *    transformer (catching pushdown/adapter drift);
 *  - warning when `controls.search.sort` has no lane for the query's target;
 *  - deciding the materialization policy (bounded scope, allowFullRepo,
 *    required vs optional) and emitting blocking diagnostics when the
 *    request is unsafe (unbounded clone).
 */
import { diagnostic } from '../diagnostics.js';
import { toGithubCodeSearchToolQuery } from '../transformers/github/code.js';
import type {
  MaterializePolicy,
  OqlDiagnostic,
  OqlPlanNode,
  OqlQuery,
  Predicate,
} from '../types.js';
import { SEARCH_SORTS_BY_TARGET } from '../types.js';

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
export function sortApplicabilityDiagnostics(query: OqlQuery): OqlDiagnostic[] {
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

export function adapterValidationDiagnostics(
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

export function decideMaterialization(
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
