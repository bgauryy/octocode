/**
 * Pagination/finish-record plumbing shared by the research-target adapters:
 * turning a backing tool's result into an `AdapterResult` (records +
 * diagnostics + pagination), the `view:"detailed"` per-domain paging window,
 * combining two pagination windows into one envelope, and the LSP semantic
 * result continuation.
 */
import { diagnostic } from '../../diagnostics.js';
import { firstQueryData } from '../runner.js';
import { toOqlPagination, type ToolPaginationPayload } from '../pagination.js';
import { packetPage } from '../graphView.js';
import type { AdapterResult } from '../local.js';
import {
  expandData,
  parentMetadata,
  records,
  sharedRepositoryRefs,
} from './rows.js';
import { errorText } from './shared.js';
import type { analyzeResearchFlow } from '../../research/analyze.js';
import type {
  OqlDiagnostic,
  OqlQuery,
  OqlRecordResultRow,
  Pagination,
  QuerySource,
} from '../../types.js';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';

export function requestedResearchMode(
  mode: unknown
): 'plan' | 'analyze' | 'prove' {
  if (mode === 'plan' || mode === 'prove') return mode;
  return 'analyze';
}

/** Domains the `view:"detailed"` research record can expand. */
const DETAILED_DOMAINS = [
  'manifests',
  'files',
  'dependencies',
  'symbols',
  'graphFacts',
] as const;
type DetailedDomain = (typeof DETAILED_DOMAINS)[number];

/**
 * Which detailed domains the agent asked for via `select`. Accepts both the bare
 * form (`select:["symbols"]`) and the dotted record-data form
 * (`select:["data.symbols"]`). Returns `undefined` when no domain selector is
 * present (→ include all domains).
 */
function requestedDetailedDomains(
  select: string[] | undefined
): ReadonlySet<DetailedDomain> | undefined {
  if (!select || select.length === 0) return undefined;
  const requested = new Set<DetailedDomain>();
  for (const raw of select) {
    const token = raw.trim();
    const bare = token.startsWith('data.') ? token.slice(5) : token;
    if ((DETAILED_DOMAINS as readonly string[]).includes(bare)) {
      requested.add(bare as DetailedDomain);
    }
  }
  return requested.size > 0 ? requested : undefined;
}

/**
 * Build the `view:"detailed"` payload as per-domain *windows* instead of whole
 * arrays (P1). Each requested domain emits a sliced `data.<domain>` window plus
 * a typed `data.<domain>Page` pagination object, all sharing the query's
 * page/itemsPerPage. Returns the combined pagination (max totalPages, OR-ed
 * hasMore) so a single `next.page` advances every detailed domain together.
 */
export function buildDetailedDomains(
  query: OqlQuery,
  data: Awaited<ReturnType<typeof analyzeResearchFlow>>
): { fields: Record<string, unknown>; pagination?: Pagination } {
  const requested = requestedDetailedDomains(query.select);
  const arrays: Record<DetailedDomain, readonly unknown[]> = {
    manifests: data.manifests,
    files: data.files,
    dependencies: data.dependencies,
    symbols: data.symbols,
    graphFacts: data.graphFacts,
  };

  const fields: Record<string, unknown> = {};
  const currentPage = Math.max(1, query.page ?? 1);
  let itemsPerPage: number | undefined;
  let maxTotalPages = 1;
  let anyMore = false;
  for (const domain of DETAILED_DOMAINS) {
    if (requested && !requested.has(domain)) continue;
    const items = arrays[domain] ?? [];
    const { packetsStart, packetsEnd, pagination } = packetPage(
      query,
      items.length
    );
    fields[domain] = items.slice(packetsStart, packetsEnd);
    fields[`${domain}Page`] = pagination;
    itemsPerPage = pagination.itemsPerPage;
    maxTotalPages = Math.max(maxTotalPages, pagination.totalPages ?? 1);
    if (pagination.hasMore) anyMore = true;
  }

  if (Object.keys(fields).length === 0) return { fields };
  return {
    fields,
    pagination: {
      currentPage,
      ...(itemsPerPage !== undefined ? { itemsPerPage } : {}),
      totalPages: maxTotalPages,
      hasMore: anyMore || currentPage < maxTotalPages,
    },
  };
}

/** Combine the packet-page window with the detailed-domain window so the
 *  envelope's `hasMore` (and thus `next.page`) reflects either having more. */
export function combinePagination(
  a: Pagination | undefined,
  b: Pagination | undefined
): Pagination | undefined {
  if (!a) return b;
  if (!b) return a;
  const currentPage = a.currentPage ?? b.currentPage;
  const itemsPerPage = a.itemsPerPage ?? b.itemsPerPage;
  return {
    ...(currentPage !== undefined ? { currentPage } : {}),
    ...(itemsPerPage !== undefined ? { itemsPerPage } : {}),
    totalPages: Math.max(a.totalPages ?? 1, b.totalPages ?? 1),
    ...(a.totalItems !== undefined ? { totalItems: a.totalItems } : {}),
    hasMore: Boolean(a.hasMore || b.hasMore),
  };
}

export function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const { status, data } = firstQueryData<{ error?: unknown }>(result);
  if (status === 'error') {
    return [
      diagnostic('invalidQuery', errorText(data?.error, `${backend} failed`), {
        backend,
      }),
    ];
  }
  if (status === 'empty') {
    return [
      diagnostic('zeroMatches', 'Query ran and matched nothing.', {
        backend,
        severity: 'info',
        blocksAnswer: false,
      }),
    ];
  }
  return [];
}

/**
 * Build an AdapterResult from a backing-tool result: map records (none on
 * error), carry status diagnostics, and emit `zeroMatches` on a clean empty so
 * an empty result is never read as silent proof.
 */
export function finishRecords(
  result: CallToolResult,
  recordType: OqlRecordResultRow['recordType'],
  backend: string,
  source?: QuerySource
): AdapterResult {
  const { data, status } = firstQueryData(result);
  const diagnostics = statusDiagnostics(result, backend);
  const items = status === 'error' ? [] : expandData(data);
  if (
    items.length === 0 &&
    !diagnostics.some(d => d.code === 'zeroMatches' || d.severity === 'error')
  ) {
    diagnostics.push(
      diagnostic('zeroMatches', `${backend} returned no results.`, {
        backend,
        severity: 'info',
        blocksAnswer: false,
      })
    );
  }
  // Promote the backing tool's pagination into the OQL envelope so run.ts can
  // emit a first-class next.page (instead of leaking raw data.next).
  const pag = (data as { pagination?: ToolPaginationPayload })?.pagination;
  const hasMore =
    pag?.hasMore === true ||
    Boolean((data as { next?: unknown })?.next) ||
    (typeof pag?.currentPage === 'number' &&
      typeof pag?.totalPages === 'number' &&
      pag.currentPage < pag.totalPages);
  const pagination = toOqlPagination(pag, hasMore);
  const shared =
    recordType === 'package' ? sharedRepositoryRefs(data) : undefined;
  return {
    results: records(items, recordType, source, parentMetadata(data)),
    ...(shared ? { shared } : {}),
    ...(pagination ? { pagination } : {}),
    diagnostics,
    provenance: [{ backend, source }],
  };
}

export function semanticPageContinuation(
  pag: {
    currentPage?: number;
    nextPage?: number;
  },
  query: OqlQuery
) {
  const nextPage =
    typeof pag.nextPage === 'number'
      ? pag.nextPage
      : typeof pag.currentPage === 'number'
        ? pag.currentPage + 1
        : (query.page ?? 1) + 1;
  return {
    query: {
      ...query,
      params: { ...(query.params ?? {}), page: nextPage },
    },
    why: 'Continue the LSP semantic result page.',
    confidence: 'exact' as const,
  };
}

export function semanticPagination(
  data: Record<string, unknown> | undefined,
  query: OqlQuery
): Pagination | undefined {
  const pag = data?.pagination as
    | {
        hasMore?: boolean;
        currentPage?: number;
        nextPage?: number;
        totalPages?: number;
        itemsPerPage?: number;
        totalItems?: number;
      }
    | undefined;
  if (!pag?.hasMore) return undefined;
  return {
    hasMore: true,
    ...(pag.currentPage !== undefined ? { currentPage: pag.currentPage } : {}),
    ...(pag.totalPages !== undefined ? { totalPages: pag.totalPages } : {}),
    ...(pag.itemsPerPage !== undefined
      ? { itemsPerPage: pag.itemsPerPage }
      : {}),
    ...(pag.totalItems !== undefined ? { totalItems: pag.totalItems } : {}),
    next: semanticPageContinuation(pag, query),
  };
}
