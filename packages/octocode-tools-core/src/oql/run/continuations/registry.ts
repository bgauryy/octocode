/**
 * Emit executable `next.*` continuations (contract Gate 10). Every continuation
 * is a full canonical OQL query runnable as-is.
 *
 * Envelope-level:
 *  - next.page      — more result pages remain
 *  - next.matchPage — per-file matches were capped
 *
 * Per-row continuations are produced by a registry keyed by row kind (and, for
 * record rows, recordType) so adding a new row's continuations is one entry,
 * never another `else if`:
 *  - code        → next.fetch (read exact content) [+ next.semantic on local]
 *  - content     → next.charRange (page the body)
 *  - materialized→ next.structure / next.files rooted at the checkpoint path
 *  - semantics   → next.fetch (read the code at a symbol location)
 *  - graph       → next.graph (bounded LSP proof for candidate graph pages)
 */
import type { AdapterResult } from '../../adapters/local.js';
import { firstScopePath } from '../../transformers/github/common.js';
import type {
  OqlContinuation,
  OqlContinuationHint,
  OqlQuery,
  OqlRecordResultRow,
  OqlResultRow,
} from '../../types.js';
import { localFileSource } from '../paths.js';
import {
  buildCodeContinuations,
  buildContentContinuations,
  buildGraphContinuations,
  buildMaterializedContinuations,
  buildResearchContinuations,
} from './records.js';
import { buildSemanticsContinuations } from './semantics.js';
import type { ContinuationCtx, RowContinuationBuilder } from './types.js';

function githubRepoLabel(query: OqlQuery): string {
  if (query.from?.kind !== 'github') return 'owner/repo';
  if (query.from.repo?.includes('/')) return query.from.repo;
  if (query.from.owner && query.from.repo) {
    return `${query.from.owner}/${query.from.repo}`;
  }
  return query.from.repo ?? 'owner/repo';
}

function githubLocalProofHint(query: OqlQuery): string {
  const repo = githubRepoLabel(query);
  const scopePath = firstScopePath(query.scope);
  const repoWithRef =
    query.from?.kind === 'github' && query.from.ref
      ? `${repo}@${query.from.ref}`
      : repo;
  const scopedRef = scopePath ? `${repo}/${scopePath}` : repo;
  const branchFlag =
    query.from?.kind === 'github' && query.from.ref
      ? ` --branch ${query.from.ref}`
      : '';

  if (scopePath) {
    return `Use \`search ${firstSearchTerm(query)} ${scopePath} --repo ${repoWithRef} --materialize required\` for one-step local proof, or \`clone ${scopedRef}${branchFlag}\` / \`cache fetch ${repo} ${scopePath}${branchFlag} --depth tree\` before retrying local search.`;
  }

  return `Choose a bounded path first with \`search ${repo} --tree\`, then use \`search ${firstSearchTerm(query)} <path> --repo ${repoWithRef} --materialize required\`, \`clone ${repo}/<path>${branchFlag}\`, or \`cache fetch ${repo} <path>${branchFlag} --depth tree\`. For deliberate whole-repo work, use \`clone ${repo}${branchFlag}\` or \`cache fetch ${repo}${branchFlag} --depth clone\`.`;
}

function firstSearchTerm(query: OqlQuery): string {
  const where = query.where;
  if (where?.kind === 'text' || where?.kind === 'regex') return where.value;
  if (where?.kind === 'structural') return 'pattern';
  return '<term>';
}

export function attachContinuations(
  query: OqlQuery,
  exec: AdapterResult
): Record<string, OqlContinuation> {
  const next: Record<string, OqlContinuation> = {};

  // Content reads page the char-window domain, not the result-row domain. The
  // per-row `next.charRange` is the executable continuation there, so never
  // emit a misleading `next.page` for target:"content".
  const hardLimitWithoutPage =
    typeof query.limit === 'number' && query.itemsPerPage === undefined;
  if (
    exec.pagination?.hasMore &&
    query.target !== 'content' &&
    !hardLimitWithoutPage
  ) {
    const filesUnit =
      exec.pagination.totalItemsKind === 'files' ||
      exec.pagination.itemUnit === 'files';
    next['next.page'] = exec.pagination.next ?? {
      query: { ...query, page: (query.page ?? 1) + 1 },
      why: filesUnit
        ? `More file pages remain (page/itemsPerPage count matched files; rows are per-match — this page holds ${exec.results.length} rows).`
        : 'More result pages remain.',
      confidence: 'exact',
    };
  }

  if (exec.diagnostics.some(d => d.code === 'matchTruncated')) {
    next['next.matchPage'] = {
      query: {
        ...query,
        controls: {
          ...query.controls,
          search: {
            ...query.controls?.search,
            matchPage: (query.controls?.search?.matchPage ?? 1) + 1,
          },
        },
      },
      why: 'Per-file matches were capped; page within files.',
      confidence: 'exact',
    };
  }

  // GitHub provider returned zero results — code search may not index this repo.
  // Emit next.materialize so agents can clone locally and retry with full coverage.
  if (
    exec.diagnostics.some(d => d.code === 'providerUnindexed') &&
    query.from?.kind === 'github' &&
    query.target === 'code'
  ) {
    next['next.materialize'] = {
      query: {
        schema: 'oql',
        target: 'materialize',
        from: query.from,
        ...(query.scope ? { scope: query.scope } : {}),
        materialize: { mode: 'required' },
      },
      why: `GitHub code search returned no results; this is not proof of absence. ${githubLocalProofHint(query)}`,
      confidence: 'heuristic',
    };
  }

  // Per-row continuations via the registry.
  const ctx: ContinuationCtx = { query, fileFrom: localFileSource(query) };
  for (const row of exec.results) {
    const key =
      row.kind === 'record'
        ? `record:${(row as OqlRecordResultRow).recordType}`
        : row.kind;
    const build = ROW_CONTINUATION_BUILDERS[key];
    if (!build) continue;
    const rowNext = build(row, ctx);
    if (rowNext && Object.keys(rowNext).length) {
      (row as { next?: Record<string, OqlContinuation> }).next = rowNext;
    }
  }
  return next;
}

export function compactRowContinuationHints(
  results: OqlResultRow[]
): Record<string, OqlContinuationHint> | undefined {
  const nextHints: Record<string, OqlContinuationHint> = {};
  let hasHints = false;

  for (const row of results) {
    const next = (row as { next?: Record<string, OqlContinuation> }).next;
    if (!next) continue;

    for (const [key, continuation] of Object.entries(next)) {
      if (!continuation.why || !continuation.confidence) continue;
      const hint = {
        why: continuation.why,
        confidence: continuation.confidence,
      };
      const existing = nextHints[key];
      if (!existing) {
        nextHints[key] = hint;
        hasHints = true;
      }

      if (!existing || hintsEqual(existing, hint)) {
        const { why, confidence, ...queryOnly } = continuation;
        void why;
        void confidence;
        next[key] = queryOnly;
      }
    }
  }

  return hasHints ? nextHints : undefined;
}

export function hintsEqual(
  left: OqlContinuationHint,
  right: OqlContinuationHint
): boolean {
  return left.why === right.why && left.confidence === right.confidence;
}

const ROW_CONTINUATION_BUILDERS: Record<string, RowContinuationBuilder> = {
  code: buildCodeContinuations,
  content: buildContentContinuations,
  'record:materialized': buildMaterializedContinuations,
  'record:semantics': buildSemanticsContinuations,
  'record:research': buildResearchContinuations,
  'record:graph': buildGraphContinuations,
};
