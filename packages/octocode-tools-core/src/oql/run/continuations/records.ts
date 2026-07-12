/**
 * Per-row continuation builders for code/content rows and record rows
 * (materialized/research/graph). See registry.ts for how these are
 * wired into ROW_CONTINUATION_BUILDERS and attached to result rows.
 */
import type {
  OqlCodeResultRow,
  OqlContinuation,
  OqlContentResultRow,
  OqlQuery,
  OqlRecordResultRow,
  OqlResultRow,
} from '../../types.js';
import { hasMissingProof } from '../proofGrades.js';
import type { ContinuationCtx } from './types.js';

function contentMatchFromQuery(
  query: OqlQuery
): NonNullable<NonNullable<OqlQuery['fetch']>['content']>['match'] | undefined {
  const where = query.where;
  if (!where) return undefined;
  if (where.kind === 'text') {
    return {
      text: where.value,
      ...(where.case === 'sensitive' ? { caseSensitive: true } : {}),
    };
  }
  if (where.kind === 'regex') {
    return {
      text: where.value,
      regex: true,
      ...(where.case === 'sensitive' ? { caseSensitive: true } : {}),
    };
  }
  return undefined;
}

export function buildCodeContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const code = row as OqlCodeResultRow;
  const from = ctx.fileFrom
    ? ctx.fileFrom(code.path)
    : (code.source ?? ctx.query.from);
  if (!from) return undefined;
  const range =
    typeof code.line === 'number'
      ? { startLine: code.line, contextLines: 2 }
      : undefined;
  const match = range ? undefined : contentMatchFromQuery(ctx.query);
  const out: Record<string, OqlContinuation> = {
    'next.fetch': {
      query: {
        schema: 'oql',
        target: 'content',
        from,
        ...(ctx.fileFrom ? {} : { scope: { path: code.path } }),
        fetch: {
          content: {
            contentView: 'none',
            ...(range ? { range } : {}),
            ...(match ? { match } : {}),
          },
        },
      },
      why: 'Read the exact content at this hit.',
      confidence: 'exact',
    },
  };
  // Semantic outline of the file. Local/materialized only: this is always
  // executable from the file anchor; a remote semantic would re-clone per hit.
  if (ctx.fileFrom) {
    out['next.semantic'] = {
      query: {
        schema: 'oql',
        target: 'semantics',
        from,
        params: { type: 'documentSymbols' },
      },
      why: 'List the semantic symbols in this file.',
      confidence: 'exact',
    };
  }
  return out;
}

export function buildContentContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const content = row as OqlContentResultRow;
  const off = content.range?.charOffset;
  if (typeof off !== 'number') return undefined;
  return {
    'next.charRange': {
      query: {
        ...ctx.query,
        fetch: {
          ...ctx.query.fetch,
          content: {
            ...ctx.query.fetch?.content,
            charOffset: off + (content.range?.charLength ?? 20000),
          },
        },
      },
      why: 'Read the next content window.',
      confidence: 'exact',
    },
  };
}

/** next.structure / next.files rooted at a derived local path. */
function localRootContinuations(
  localPath: string,
  label: string
): Record<string, OqlContinuation> {
  const from = { kind: 'local' as const, path: localPath };
  return {
    'next.structure': {
      query: { schema: 'oql', target: 'structure', from },
      why: `List the ${label} tree.`,
      confidence: 'exact',
    },
    'next.files': {
      query: { schema: 'oql', target: 'files', from },
      why: `Enumerate files in the ${label}.`,
      confidence: 'exact',
    },
  };
}

function derivedLocalPath(row: OqlResultRow): string | undefined {
  const data = (row as OqlRecordResultRow).data;
  return typeof data?.localPath === 'string' ? data.localPath : undefined;
}

export function buildMaterializedContinuations(
  row: OqlResultRow
): Record<string, OqlContinuation> | undefined {
  const lp = derivedLocalPath(row);
  return lp ? localRootContinuations(lp, 'materialized') : undefined;
}

/**
 * `target:"research"` stays candidate-grade — it never runs LSP internally — but
 * it emits a one-call `next.graph` upgrade. `target:"graph"` emits the same
 * upgrade when an analyze row still has missing proof. The upgrade is page-aligned
 * and bounded by `proofLimit`, so agents can close proof without losing the
 * research/graph honesty boundary.
 */
export function buildResearchContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  return graphProofUpgrade(row, ctx, {
    why: 'Upgrade this candidate research to LSP-proven relationships for the current page (bounded proof).',
    force: true,
  });
}

export function buildGraphContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const params = ctx.query.params ?? {};
  if (
    params.proof === 'none' ||
    params.proof === 'lsp' ||
    params.mode === 'prove'
  ) {
    return undefined;
  }
  const data = (row as OqlRecordResultRow).data;
  if (!hasMissingProof(data)) return undefined;
  return graphProofUpgrade(row, ctx, {
    why: 'Upgrade this candidate graph page to LSP-proven relationships (bounded proof).',
  });
}

function graphProofUpgrade(
  row: OqlResultRow,
  ctx: ContinuationCtx,
  options: { why: string; force?: boolean }
): Record<string, OqlContinuation> | undefined {
  const from = ctx.query.from;
  // Graph proof needs a complete local file universe (local/materialized).
  if (from?.kind !== 'local' && from?.kind !== 'materialized') return undefined;
  const data = (row as OqlRecordResultRow).data;
  if (!options.force && !hasMissingProof(data)) return undefined;
  const intent =
    typeof data?.intent === 'string' && data.intent.length > 0
      ? data.intent
      : typeof ctx.query.params?.intent === 'string' &&
          ctx.query.params.intent.length > 0
        ? ctx.query.params.intent
        : 'reachability';
  // proofLimit is bounded (graphParams caps at 25); align it to the page size so
  // the upgrade proves roughly the same number of subjects shown this page.
  const proofLimit = Math.min(25, Math.max(1, ctx.query.itemsPerPage ?? 10));
  const params = ctx.query.params ?? {};
  const facets = continuationResearchFacets(data, params);
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
          ...(facets ? { facets } : {}),
        },
        ...(ctx.query.page ? { page: ctx.query.page } : {}),
        ...(ctx.query.itemsPerPage
          ? { itemsPerPage: ctx.query.itemsPerPage }
          : {}),
      },
      why: options.why,
      confidence: 'exact',
    },
  };
}

const PUBLIC_RESEARCH_FACETS = new Set([
  'symbols',
  'files',
  'dependencies',
  'relations',
]);

function continuationResearchFacets(
  data: Record<string, unknown>,
  params: Record<string, unknown>
): string[] | undefined {
  const raw = Array.isArray(params.facets)
    ? params.facets
    : Array.isArray(data.facets)
      ? data.facets
      : undefined;
  if (!raw) return undefined;
  const facets = raw.filter(
    (facet): facet is string =>
      typeof facet === 'string' && PUBLIC_RESEARCH_FACETS.has(facet)
  );
  return facets.length > 0 ? facets : undefined;
}
