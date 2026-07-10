/**
 * Research-target adapters: semantics (LSP), repositories, packages,
 * pullRequests, commits, artifacts, diff, and smart research packets.
 *
 * Each compiles a canonical OQL query (from + scope + `params` bag) into the
 * existing bulk tool runner and maps the single query's `data` payload into
 * generic record rows. Remote semantics route through materialization first
 * (clone → local LSP). This keeps the planner/dispatch uniform; per-target
 * specifics live behind one `params` bag validated by the backing tool.
 */
import { statSync } from 'node:fs';
import nodePath from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect, firstQueryData, stringFrom } from './runner.js';
import { toOqlPagination, type ToolPaginationPayload } from './pagination.js';
import { diagnostic } from '../diagnostics.js';
import { classifyDiffLane } from '../diffLanes.js';
import { spawnWithTimeout } from '../../utils/exec/spawn.js';
import { toGithubRepositoryLanguage } from '../transformers/language.js';
import { analyzeResearchFlow } from '../research/analyze.js';
import { buildResearchPackets } from '../research/packets.js';
import {
  buildGraphView,
  graphFilters,
  nativeGraphSummary,
  packetPage,
  summarizePacketGraph,
} from './graphView.js';
import {
  escalateGraphPacketsWithLsp,
  graphProofLimit,
  shouldRunLspProof,
} from './graphProof.js';
import type { AdapterResult } from './local.js';
import type {
  OqlGraphData,
  OqlDiagnostic,
  OqlContinuation,
  Pagination,
  OqlQuery,
  OqlRecordResultRow,
  QuerySource,
} from '../types.js';

/* ------------------------------ helpers --------------------------------- */

/**
 * Pull file content/status/error out of a ghGetFileContent (or localGetFileContent)
 * result. The row sits directly under structuredContent.results[0] with the file
 * in files[0] (no nested `.data`); some shapes nest under `.data` or `.results`.
 * Used by the direct two-ref diff lanes — reading `.data.content` is always
 * undefined for this tool and previously masqueraded as "files identical".
 */
function ghFileContentResult(result: CallToolResult): {
  content?: string;
  status?: string;
  error?: unknown;
} {
  const sc = result.structuredContent as
    { results?: Array<Record<string, unknown>> } | undefined;
  const row = sc?.results?.[0];
  if (!row) return {};
  const data = ('data' in row ? row.data : row) as
    Record<string, unknown> | undefined;
  const fileRow =
    (data?.files as Array<Record<string, unknown>> | undefined)?.[0] ??
    (data?.results as Array<Record<string, unknown>> | undefined)?.[0] ??
    data ??
    {};
  const content = fileRow.content;
  return {
    content: typeof content === 'string' ? content : undefined,
    status: row.status as string | undefined,
    error: fileRow.error ?? data?.error ?? row.error,
  };
}

function errorText(value: unknown, fallback: string): string {
  if (typeof value === 'string' && value.trim()) return value;
  if (value instanceof Error && value.message) return value.message;
  if (value && typeof value === 'object') {
    const record = value as { error?: unknown; message?: unknown };
    if (typeof record.error === 'string' && record.error.trim()) {
      return record.error;
    }
    if (typeof record.message === 'string' && record.message.trim()) {
      return record.message;
    }
    const serialized = JSON.stringify(value);
    if (serialized) return serialized;
  }
  return fallback;
}

function materializedClonePath(
  result: CallToolResult,
  localPath: string | undefined
): string | undefined {
  if (!localPath || nodePath.isAbsolute(localPath)) return localPath;
  const sc = result.structuredContent as { base?: string } | undefined;
  return sc?.base ? nodePath.join(sc.base, localPath) : localPath;
}

function resolveSemanticSourceUri(
  reportedUri: string | undefined,
  fallbackUri: string | undefined,
  workspaceRoot: string | undefined
): string {
  const fallback = fallbackUri ?? workspaceRoot ?? '.';
  if (!reportedUri) return fallback;
  if (nodePath.isAbsolute(reportedUri)) return reportedUri;

  if (workspaceRoot && nodePath.isAbsolute(workspaceRoot)) {
    return nodePath.resolve(workspaceRoot, reportedUri);
  }

  if (fallbackUri && nodePath.isAbsolute(fallbackUri)) {
    const base = isExistingDirectory(fallbackUri)
      ? fallbackUri
      : nodePath.dirname(fallbackUri);
    return nodePath.resolve(base, reportedUri);
  }

  return reportedUri;
}

function firstScopePath(query: OqlQuery): string | undefined {
  const path = query.scope?.path;
  return Array.isArray(path) ? path[0] : path;
}

/** Known array-valued payload fields, in priority order. */
const RECORD_ARRAY_KEYS = [
  'repositories',
  'pull_requests',
  'commits',
  'packages',
  'results',
  'locations',
  'references',
  'symbols',
  'strings',
  'entries',
  'incomingCalls',
  'outgoingCalls',
];

const RECORD_PARENT_METADATA_EXCLUDE = new Set([
  ...RECORD_ARRAY_KEYS,
  'pagination',
  'contentPagination',
  'next',
]);

/** Expand a tool `data` payload into row items (an inner array if present). */
function expandData(data: Record<string, unknown> | undefined): unknown[] {
  if (!data) return [];
  for (const key of RECORD_ARRAY_KEYS) {
    const v = (data as Record<string, unknown>)[key];
    if (Array.isArray(v)) return v;
  }
  return [data];
}

function sharedRepositoryRefs(
  parent: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  const repositories = parent?.repositories;
  if (
    !repositories ||
    typeof repositories !== 'object' ||
    Array.isArray(repositories)
  ) {
    return undefined;
  }

  const compact: Record<string, unknown> = {};
  for (const [id, value] of Object.entries(repositories)) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) continue;
    const source = value as Record<string, unknown>;
    const repo: Record<string, unknown> = {};
    for (const key of ['repository', 'repositoryDirectory', 'owner', 'repo']) {
      if (typeof source[key] === 'string') repo[key] = source[key];
    }
    if (Object.keys(repo).length > 0) compact[id] = repo;
  }

  return Object.keys(compact).length ? { repositories: compact } : undefined;
}

function parentMetadata(
  data: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!data) return undefined;
  const metadata: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(data)) {
    if (RECORD_PARENT_METADATA_EXCLUDE.has(key)) continue;
    if (
      value === null ||
      typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean'
    ) {
      metadata[key] = value;
    }
  }
  return Object.keys(metadata).length ? metadata : undefined;
}

function records(
  items: unknown[],
  recordType: OqlRecordResultRow['recordType'],
  source?: QuerySource,
  metadata?: Record<string, unknown>
): OqlRecordResultRow[] {
  return items.map(item => {
    const data = (
      item && typeof item === 'object'
        ? (item as Record<string, unknown>)
        : { value: item }
    ) as Record<string, unknown>;
    const id = stableId(recordType, data);
    return {
      kind: 'record' as const,
      recordType,
      ...(id ? { id } : {}),
      ...(source ? { source } : {}),
      ...(metadata ? { metadata } : {}),
      data,
    };
  });
}

function requestedResearchMode(mode: unknown): 'plan' | 'analyze' | 'prove' {
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
function buildDetailedDomains(
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
function combinePagination(
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Citeable identity per record type, extracted from the backend payload. */
function stableId(
  recordType: OqlRecordResultRow['recordType'],
  d: Record<string, unknown>
): string | undefined {
  const s = (k: string): string | undefined =>
    typeof d[k] === 'string' || typeof d[k] === 'number'
      ? String(d[k])
      : undefined;
  switch (recordType) {
    case 'repository':
      return (
        s('fullName') ??
        (s('owner') && s('repo') ? `${s('owner')}/${s('repo')}` : s('url')) ??
        valueLeadingToken(d)
      );
    case 'package': {
      const name = s('name') ?? s('packageName');
      const ver = s('version');
      return name ? (ver ? `${name}@${ver}` : name) : valueLeadingToken(d);
    }
    case 'pullRequest':
      return s('number')
        ? `#${s('number')}`
        : (s('url') ?? valueLeadingToken(d));
    case 'commit':
      return (
        s('sha')?.slice(0, 12) ?? s('oid')?.slice(0, 12) ?? valueLeadingToken(d)
      );
    case 'artifact':
      return s('localPath') ?? s('path');
    case 'materialized':
      return s('localPath') ?? s('repoRoot');
    case 'diff':
      // Whole-PR patch rows have no single path — cite the PR number instead.
      return (
        s('path') ??
        s('filename') ??
        (s('number') ? `#${s('number')}` : valueLeadingToken(d))
      );
    case 'semantics': {
      const uri = s('uri');
      const line = s('line') ?? s('startLine');
      return uri ? (line ? `${uri}:${line}` : uri) : undefined;
    }
    case 'research':
      return s('intent') ?? s('goal') ?? 'research';
    case 'graph':
      return s('intent') ? `graph:${s('intent')}` : 'graph';
  }
  return valueLeadingToken(d);
}

/**
 * Concise lanes flatten rows to `{ value: "<id> <title…>" }` (e.g. PR rows
 * become "#3536 chore(...)"); keep a citeable identity from the leading token
 * instead of dropping the id entirely.
 */
function valueLeadingToken(d: Record<string, unknown>): string | undefined {
  return typeof d.value === 'string' && d.value.trim()
    ? d.value.trim().split(/\s+/, 1)[0]
    : undefined;
}

function statusDiagnostics(
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

function isExistingDirectory(path: string): boolean {
  try {
    const resolved = nodePath.isAbsolute(path) ? path : nodePath.resolve(path);
    return statSync(resolved).isDirectory();
  } catch {
    return false;
  }
}

function splitRepo(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (!source || source.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

function params(query: OqlQuery): Record<string, unknown> {
  return query.params ?? {};
}

function firstScopeLanguage(query: OqlQuery): string | undefined {
  const lang = query.scope?.language;
  if (!lang) return undefined;
  return Array.isArray(lang) ? lang[0] : lang;
}

function withOqlPaging(
  query: OqlQuery,
  limitKey?: 'limit' | 'perPage'
): Record<string, unknown> {
  const out: Record<string, unknown> = { ...params(query) };
  if (out.page === undefined && query.page !== undefined) {
    out.page = query.page;
  }
  if (limitKey && out[limitKey] === undefined) {
    const limit = query.limit ?? query.itemsPerPage;
    if (limit !== undefined) out[limitKey] = limit;
  }
  return out;
}

/**
 * Build an AdapterResult from a backing-tool result: map records (none on
 * error), carry status diagnostics, and emit `zeroMatches` on a clean empty so
 * an empty result is never read as silent proof.
 */
function finishRecords(
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

// references/callers are bounded by the server's open-file set. The tool
// auto-opens a bounded set of name-matching consumer files before relation
// queries, but a zero is still candidate evidence, not deletion-grade proof.
function zeroSemanticResultDiagnostic(): OqlDiagnostic {
  return diagnostic(
    'partialResult',
    'Zero LSP results after bounded consumer warm-up — still not proof of unused. Cross-check with a text search (target:"code") for dynamic, re-exported, or string-based usage before concluding.',
    { backend: 'lspGetSemantics', blocksAnswer: true }
  );
}

function semanticDiagnostics(
  data: Record<string, unknown> | undefined,
  query: OqlQuery
): OqlDiagnostic[] {
  const diagnostics: OqlDiagnostic[] = [];
  const lsp = data?.lsp as
    { serverAvailable?: boolean; source?: string } | undefined;
  if (lsp?.serverAvailable === false) {
    diagnostics.push(
      diagnostic(
        'lspUnavailable',
        lsp.source === 'native'
          ? 'Language server was unavailable; native fallback returned partial semantic data.'
          : 'Language server was unavailable; semantic proof is incomplete.',
        { backend: 'lspGetSemantics' }
      )
    );
  }

  const payload = data?.payload as
    | {
        kind?: string;
        category?: string;
        reason?: string;
        totalReferences?: number;
        incomingCalls?: number;
        outgoingCalls?: number;
      }
    | undefined;
  if (payload?.kind === 'empty') {
    if (
      payload.category === 'symbolNotFound' ||
      payload.category === 'anchorFailed'
    ) {
      // The anchor never resolved: this is a miss, not an empty answer. It
      // must not surface as "0 references" proof — a typo'd symbolName would
      // be indistinguishable from provably-unreferenced.
      diagnostics.push(
        diagnostic(
          'symbolNotFound',
          `${payload.reason ?? 'Symbol anchor resolution failed.'} Refresh the lineHint from a search/AST anchor and retry.`,
          { backend: 'lspGetSemantics' }
        )
      );
    } else if (
      payload.category === 'noReferences' ||
      payload.category === 'noCalls'
    ) {
      diagnostics.push(zeroSemanticResultDiagnostic());
    }
  } else if (
    (payload?.kind === 'references' && payload.totalReferences === 0) ||
    (payload?.kind === 'callers' && payload.incomingCalls === 0) ||
    (payload?.kind === 'callees' && payload.outgoingCalls === 0) ||
    (payload?.kind === 'callHierarchy' &&
      payload.incomingCalls === 0 &&
      payload.outgoingCalls === 0)
  ) {
    diagnostics.push(zeroSemanticResultDiagnostic());
  }

  const pag = data?.pagination as
    | {
        hasMore?: boolean;
        currentPage?: number;
        nextPage?: number;
        totalPages?: number;
      }
    | undefined;
  if (pag?.hasMore) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        'Semantic result is paginated; follow the continuation before treating it as complete proof.',
        {
          backend: 'lspGetSemantics',
          blocksAnswer: true,
          continuation: semanticPageContinuation(pag, query),
        }
      )
    );
  }
  return diagnostics;
}

function semanticPageContinuation(
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

function semanticPagination(
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

/* --------------------------- target adapters ---------------------------- */

export async function executeRepositories(
  query: OqlQuery
): Promise<AdapterResult> {
  const { owner } = splitRepo(query.from);
  const forwarded = withOqlPaging(query, 'limit');
  const rawLanguage =
    typeof forwarded.language === 'string'
      ? forwarded.language
      : firstScopeLanguage(query);
  const language = toGithubRepositoryLanguage(rawLanguage);
  if (language) forwarded.language = language;
  const result = await runDirect('ghSearchRepos', {
    ...(owner ? { owner } : {}),
    ...forwarded,
  });
  const finished = finishRecords(
    result,
    'repository',
    'ghSearchRepos',
    query.from ?? { kind: 'github' }
  );
  // GitHub repo search ANDs every term across name/description/readme, so a
  // multi-term zero is usually over-constraint, not absence — say so instead
  // of letting "0 results, proof" read as a settled answer.
  if (finished.results.length === 0 && multiTermRepoQuery(forwarded)) {
    finished.diagnostics.push(
      diagnostic(
        'zeroMatches',
        'GitHub repository search requires EVERY term to match (AND semantics). Zero results for a multi-term query usually means over-constraint, not absence.',
        {
          backend: 'ghSearchRepos',
          severity: 'info',
          blocksAnswer: false,
          repair: {
            message:
              'Retry with the single most distinctive term (e.g. the project name), or move concepts to topic:"..." filters.',
          },
        }
      )
    );
  }
  return finished;
}

function multiTermRepoQuery(forwarded: Record<string, unknown>): boolean {
  // Shorthand lowers the positional text to `keywords` (term-split); raw
  // callers may pass `keywordsToSearch`. Either way, >1 term (or one term
  // containing spaces) means provider-AND over-constraint is in play.
  const terms = forwarded.keywords ?? forwarded.keywordsToSearch;
  if (Array.isArray(terms)) {
    return (
      terms.length > 1 ||
      (terms.length === 1 &&
        typeof terms[0] === 'string' &&
        terms[0].trim().includes(' '))
    );
  }
  return typeof terms === 'string' && terms.trim().includes(' ');
}

export async function executePackages(query: OqlQuery): Promise<AdapterResult> {
  const result = await runDirect('npmSearch', { ...withOqlPaging(query) });
  return finishRecords(
    result,
    'package',
    'npmSearch',
    query.from ?? { kind: 'npm' }
  );
}

export async function executeHistory(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(query.from);
  const commits = query.target === 'commits';

  // P4: `matchString` is an OQL-layer *content* filter applied to fetched
  // bodies — never a backing search-index claim. Strip it (and matchScope) from
  // the params forwarded to ghHistoryResearch for BOTH lanes so the tool is not
  // asked to interpret it as a query field, then apply it client-side with
  // honest partial/zero-match diagnostics. (Commits previously forwarded
  // matchString raw and never filtered — a silent drop if the backend ignored
  // it; PRs and commits now share the same content-filter discipline.)
  const pr = !commits ? pullRequestMatch(query) : undefined;
  const commitNeedle = commits ? commitMatchNeedle(query) : undefined;
  const forwarded = withOqlPaging(query, commits ? 'perPage' : 'limit');
  if (pr || commitNeedle) {
    delete forwarded.matchString;
    delete forwarded.matchScope;
  }

  const result = await runDirect('ghHistoryResearch', {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    ...(commits ? { type: 'commits' } : {}),
    ...forwarded,
  });
  const mapped = finishRecords(
    result,
    commits ? 'commit' : 'pullRequest',
    'ghHistoryResearch',
    query.from ?? { kind: 'github' }
  );
  if (pr) return filterPullRequestsByMatch(mapped, pr);
  if (commitNeedle) return filterCommitsByMatch(mapped, commitNeedle);
  return mapped;
}

/** Read the validated commit content-match needle, if present. */
function commitMatchNeedle(query: OqlQuery): string | undefined {
  const p = params(query);
  return typeof p.matchString === 'string' && p.matchString.length > 0
    ? p.matchString
    : undefined;
}

/**
 * Keep only commit records whose message contains `needle` (case-insensitive
 * substring), spotlight where it matched, and surface honest diagnostics — a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 * Mirrors {@link filterPullRequestsByMatch}; commit text is the commit message.
 */
export function filterCommitsByMatch(
  result: AdapterResult,
  needle: string
): AdapterResult {
  const needleLower = needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const messageVal = (data as Record<string, unknown>).message;
    const haystack = typeof messageVal === 'string' ? messageVal : '';
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: needle,
      scope: 'message',
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No commit message matched "${needle}" (content filter over ${total} fetched commit(s); not a search-index query). Broaden the fetch (branch/perPage/page).`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched commit(s) matching "${needle}" in message. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}

export interface PullRequestMatch {
  needle: string;
  scope: 'body' | 'title' | 'comments' | 'reviews' | 'all';
}

/** Read the validated PR content-match params, if present. */
function pullRequestMatch(query: OqlQuery): PullRequestMatch | undefined {
  const p = params(query);
  const needle = typeof p.matchString === 'string' ? p.matchString : undefined;
  if (!needle) return undefined;
  const scope =
    p.matchScope === 'title' ||
    p.matchScope === 'comments' ||
    p.matchScope === 'reviews' ||
    p.matchScope === 'all'
      ? p.matchScope
      : 'body';
  return { needle, scope };
}

/** Collect the searchable text for a PR record under the requested scope. */
function pullRequestScopeText(
  data: Record<string, unknown>,
  scope: PullRequestMatch['scope']
): string {
  const parts: string[] = [];
  const push = (v: unknown) => {
    if (typeof v === 'string' && v.length > 0) parts.push(v);
  };
  const bodies = (key: string) => {
    const list = data[key];
    if (Array.isArray(list)) {
      for (const c of list) {
        if (c && typeof c === 'object') push((c as { body?: unknown }).body);
      }
    }
  };
  if (scope === 'body' || scope === 'all') push(data.body);
  if (scope === 'title' || scope === 'all') push(data.title);
  if (scope === 'comments' || scope === 'all') bodies('comments');
  if (scope === 'reviews' || scope === 'all') bodies('reviews');
  return parts.join('\n');
}

/**
 * Keep only PR records whose scope text contains `matchString` (case-insensitive
 * substring), spotlight where each matched, and surface honest diagnostics: a
 * `partialResult` when some were dropped, `zeroMatches` when none matched.
 */
export function filterPullRequestsByMatch(
  result: AdapterResult,
  match: PullRequestMatch
): AdapterResult {
  const needleLower = match.needle.toLowerCase();
  const total = result.results.length;
  const kept = result.results.filter(row => {
    if (row.kind !== 'record') return false;
    const data = (row as OqlRecordResultRow).data;
    const haystack = pullRequestScopeText(data, match.scope);
    const idx = haystack.toLowerCase().indexOf(needleLower);
    if (idx < 0) return false;
    // Additive spotlight: a bounded window around the first hit (full body/
    // comment text is left intact on the record).
    const start = Math.max(0, idx - 80);
    const end = Math.min(haystack.length, idx + match.needle.length + 80);
    (data as Record<string, unknown>).match = {
      matchString: match.needle,
      scope: match.scope,
      spotlight:
        (start > 0 ? '…' : '') +
        haystack.slice(start, end) +
        (end < haystack.length ? '…' : ''),
    };
    return true;
  });

  const diagnostics = result.diagnostics.filter(d => d.code !== 'zeroMatches');
  if (kept.length === 0) {
    diagnostics.push(
      diagnostic(
        'zeroMatches',
        `No pull request ${match.scope} matched "${match.needle}" (content filter over ${total} fetched PR(s); not a search-index query). Broaden the fetch (state/keywordsToSearch/page) or the match scope.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  } else if (kept.length < total) {
    diagnostics.push(
      diagnostic(
        'partialResult',
        `Content filter kept ${kept.length} of ${total} fetched PR(s) matching "${match.needle}" in ${match.scope}. This filters fetched content only — page the fetch to widen the candidate set.`,
        { backend: 'ghHistoryResearch', severity: 'info', blocksAnswer: false }
      )
    );
  }

  return { ...result, results: kept, diagnostics };
}

/**
 * `target:"diff"` has two typed lanes, discriminated by params shape:
 *   - PR patch:    { prNumber, files? }            -> ghHistoryResearch patches
 *   - direct file: { baseRef, headRef, path }      -> two ghGetFileContent reads
 *                                                     + a pure local line diff
 * A request that fits neither returns a repair diagnostic rather than silently
 * falling through to a PR-patch call.
 */
export async function executeDiff(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const { owner, repo } = splitRepo(query.from);
  // Lane discriminant is shared with the planner (diffLanes.ts) — one source of
  // truth, so dry-run plan and execution can never disagree.
  const lane = classifyDiffLane(p);

  if (lane.kind === 'prPatch') {
    // PR patch lane (unchanged behavior).
    const result = await runDirect('ghHistoryResearch', {
      ...(owner ? { owner } : {}),
      ...(repo ? { repo } : {}),
      content: { patches: { mode: 'all' } },
      ...p,
    });
    return finishRecords(
      result,
      'diff',
      'ghHistoryResearch',
      query.from ?? { kind: 'github' }
    );
  }

  if (lane.kind === 'directFile') {
    if (query.from?.kind === 'local' || query.from?.kind === 'materialized') {
      return executeLocalDirectFileDiff(query, {
        baseRef: lane.baseRef,
        headRef: lane.headRef,
        path: lane.path,
      });
    }
    return executeDirectFileDiff(query, owner, repo, {
      baseRef: lane.baseRef,
      headRef: lane.headRef,
      path: lane.path,
    });
  }

  return {
    results: [],
    diagnostics: [
      diagnostic(
        'invalidQuery',
        'target:"diff" needs either {prNumber} (PR patch diff) or {baseRef,headRef,path} (direct file diff between two refs).',
        {
          backend: 'ghHistoryResearch',
          repair: {
            message:
              'Add params.prNumber for a PR patch, or params.baseRef + params.headRef + params.path for a direct file diff.',
          },
        }
      ),
    ],
    provenance: [],
  };
}

/** Direct two local files via two content reads + a pure local line diff. */
// Git refs the diff lane will pass to `git show` — conservative shape, and
// never starting with '-' so a ref can't be parsed as an option.
const SAFE_GIT_REF = /^[A-Za-z0-9][A-Za-z0-9._/@^~-]*$/;

async function executeLocalDirectFileDiff(
  query: OqlQuery,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  const source = query.from;
  const basePath =
    source?.kind === 'local'
      ? source.path
      : source?.kind === 'materialized'
        ? source.localPath
        : undefined;
  if (!basePath) {
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', 'Local direct file diff needs from.path.', {
          backend: 'localGetFileContent',
        }),
      ],
      provenance: [],
    };
  }

  const invalid = (message: string): AdapterResult => ({
    results: [],
    diagnostics: [diagnostic('invalidQuery', message, { backend: 'git' })],
    provenance: [{ backend: 'git', source: query.from }],
  });

  // The lane contract is "path at baseRef vs path at headRef", so both sides
  // come from git object storage — not from files on disk (the worktree may
  // hold neither ref's version).
  const gitCwd = isExistingDirectory(basePath)
    ? basePath
    : nodePath.dirname(basePath);
  const rel = nodePath.isAbsolute(refs.path)
    ? nodePath.relative(gitCwd, refs.path)
    : refs.path;
  if (!rel || rel.startsWith('..') || rel.startsWith('-')) {
    return invalid(
      `params.path must resolve inside from.path for a local ref diff (got "${refs.path}").`
    );
  }
  if (!SAFE_GIT_REF.test(refs.baseRef) || !SAFE_GIT_REF.test(refs.headRef)) {
    return invalid(
      'baseRef/headRef must be plain git revisions (branch, tag, sha, HEAD~N).'
    );
  }

  // `ref:path` is repo-root-relative in git; the `./` prefix makes it
  // cwd-relative so from.path anchors the lookup as documented.
  const relPosix = `./${rel.split(nodePath.sep).join('/')}`;
  const show = (ref: string) =>
    spawnWithTimeout('git', ['-C', gitCwd, 'show', `${ref}:${relPosix}`], {
      timeout: 15_000,
    });
  const [base, head] = await Promise.all([
    show(refs.baseRef),
    show(refs.headRef),
  ]);
  if (!base.success || !head.success) {
    const failed = !base.success ? base : head;
    const ref = !base.success ? refs.baseRef : refs.headRef;
    return invalid(
      `git show ${ref}:${relPosix} failed: ${failed.stderr.trim().split('\n')[0] || failed.error?.message || `exit ${failed.exitCode}`}. from.path must be inside a git repository and the path must exist at both refs.`
    );
  }

  const diff = computeLineDiff(base.stdout, head.stdout);
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: `${refs.baseRef}..${refs.headRef}:${rel}`,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: rel,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };

  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic(
              'zeroMatches',
              `${rel} is identical at ${refs.baseRef} and ${refs.headRef}.`,
              { backend: 'git', severity: 'info', blocksAnswer: false }
            ),
          ]
        : [],
    provenance: [{ backend: 'git', source: query.from }],
  };
}

/** Direct two-ref GitHub file diff via two content reads + a pure local line diff. */
async function executeDirectFileDiff(
  query: OqlQuery,
  owner: string | undefined,
  repo: string | undefined,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  if (!owner || !repo) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'Direct file diff needs a concrete owner/repo.',
          { backend: 'ghGetFileContent' }
        ),
      ],
      provenance: [],
    };
  }

  const read = (ref: string) =>
    runDirect('ghGetFileContent', {
      owner,
      repo,
      path: refs.path,
      branch: ref,
      fullContent: true,
      minify: 'none',
    });

  const [baseRes, headRes] = await Promise.all([
    read(refs.baseRef),
    read(refs.headRef),
  ]);

  // ghGetFileContent returns the row directly under structuredContent.results[0]
  // (keys: id/owner/repo/files) with the file content in files[0].content — there
  // is no nested `.data`, so firstQueryData(...).data is empty. Reading it as
  // `.content` was always undefined, which previously masqueraded as "identical".
  const base = ghFileContentResult(baseRes);
  const head = ghFileContentResult(headRes);
  const unresolvedRef = [
    { label: 'base', ref: refs.baseRef, ...base },
    { label: 'head', ref: refs.headRef, ...head },
  ].find(item => item.status === 'error' || typeof item.content !== 'string');

  if (unresolvedRef) {
    const err = errorText(
      unresolvedRef.error,
      `Could not read ${unresolvedRef.label} ref "${unresolvedRef.ref}" for ${refs.path}.`
    );
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', err, { backend: 'ghGetFileContent' }),
      ],
      provenance: [{ backend: 'ghGetFileContent', source: query.from }],
    };
  }

  const diff = computeLineDiff(base.content ?? '', head.content ?? '');
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: refs.path,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: refs.path,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };
  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic('zeroMatches', 'Files are identical at both refs.', {
              backend: 'ghGetFileContent',
              severity: 'info',
              blocksAnswer: false,
            }),
          ]
        : [],
    provenance: [{ backend: 'ghGetFileContent', source: query.from }],
  };
}

export interface LineDiff {
  additions: number;
  deletions: number;
  unchanged: number;
  /** Unified-style patch text (`+`/`-`/` ` line prefixes). */
  patch: string;
}

/**
 * Minimal LCS-based line diff between two file bodies. Pure and dependency-free
 * so it is unit-testable without any backend. Not a byte-perfect git patch —
 * a line-granular additions/deletions view for direct two-ref comparison.
 */
export function computeLineDiff(baseText: string, headText: string): LineDiff {
  const a = baseText === '' ? [] : baseText.split('\n');
  const b = headText === '' ? [] : headText.split('\n');
  const n = a.length;
  const m = b.length;

  // LCS length table.
  const lcs: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0)
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j]
          ? lcs[i + 1]![j + 1]! + 1
          : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
    }
  }

  const lines: string[] = [];
  let additions = 0;
  let deletions = 0;
  let unchanged = 0;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      lines.push(`  ${a[i]}`);
      unchanged++;
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      lines.push(`- ${a[i]}`);
      deletions++;
      i++;
    } else {
      lines.push(`+ ${b[j]}`);
      additions++;
      j++;
    }
  }
  while (i < n) {
    lines.push(`- ${a[i++]}`);
    deletions++;
  }
  while (j < m) {
    lines.push(`+ ${b[j++]}`);
    additions++;
  }

  return { additions, deletions, unchanged, patch: lines.join('\n') };
}

export async function executeArtifacts(
  query: OqlQuery
): Promise<AdapterResult> {
  const path =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;
  if (!path) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"artifacts" needs a local file `from` (path).',
          { backend: 'localBinaryInspect' }
        ),
      ],
      provenance: [],
    };
  }
  const result = await runDirect('localBinaryInspect', {
    path,
    ...params(query),
  });
  // An artifact is a single entity: keep ONE record row carrying the full
  // payload (mode, entries/strings/symbols, derived localPath, nextScanOffset)
  // rather than expanding inner arrays into rows — otherwise parent-level
  // metadata (localPath, scan cursor) is lost to the continuation builders.
  const { data, status } = firstQueryData(result);
  const diagnostics = statusDiagnostics(result, 'localBinaryInspect');
  if (status === 'error' || !data) {
    return {
      results: [],
      diagnostics: diagnostics.length
        ? diagnostics
        : [
            diagnostic('zeroMatches', 'localBinaryInspect returned no data.', {
              backend: 'localBinaryInspect',
              severity: 'info',
              blocksAnswer: false,
            }),
          ],
      provenance: [{ backend: 'localBinaryInspect', source: query.from }],
    };
  }
  return {
    results: records([data], 'artifact', query.from),
    diagnostics: [...diagnostics, ...artifactPartialDiagnostics(data, query)],
    provenance: [{ backend: 'localBinaryInspect', source: query.from }],
  };
}

type ArtifactTextPagination = {
  hasMore?: boolean;
  nextCharOffset?: number;
  charLength?: number;
};

function artifactPartialDiagnostics(
  data: Record<string, unknown>,
  query: OqlQuery
): OqlDiagnostic[] {
  const pagination =
    data.pagination && typeof data.pagination === 'object'
      ? (data.pagination as ArtifactTextPagination)
      : undefined;
  if (data.isPartial !== true && pagination?.hasMore !== true) return [];
  return [
    diagnostic(
      'partialResult',
      'Artifact text is paginated; follow the artifact continuation before treating the inline content as complete.',
      {
        backend: 'localBinaryInspect',
        blocksAnswer: true,
        continuation: artifactContentContinuation(query, pagination),
      }
    ),
  ];
}

function artifactContentContinuation(
  query: OqlQuery,
  pagination: ArtifactTextPagination | undefined
): OqlContinuation | undefined {
  if (
    pagination?.hasMore !== true ||
    typeof pagination.nextCharOffset !== 'number'
  ) {
    return undefined;
  }
  return {
    query: {
      ...query,
      params: {
        ...(query.params ?? {}),
        charOffset: pagination.nextCharOffset,
        ...(typeof pagination.charLength === 'number'
          ? { charLength: pagination.charLength }
          : {}),
      },
    },
    why: 'Read the next inline artifact text window.',
    confidence: 'exact',
  };
}

export async function executeSemantics(
  query: OqlQuery
): Promise<AdapterResult> {
  let uri: string | undefined;
  let workspaceRoot: string | undefined;
  const provenance: AdapterResult['provenance'] = [];
  const diagnostics: OqlDiagnostic[] = [];
  const semanticParams = params(query) as {
    uri?: string;
    type?: string;
    workspaceRoot?: string;
  } & Record<string, unknown>;
  const isWorkspaceSymbol = semanticParams.type === 'workspaceSymbol';
  const explicitUri =
    typeof semanticParams.uri === 'string' ? semanticParams.uri : undefined;
  const explicitWorkspaceRoot =
    typeof semanticParams.workspaceRoot === 'string'
      ? semanticParams.workspaceRoot
      : undefined;

  if (query.from?.kind === 'local') {
    if (isWorkspaceSymbol) {
      const fromPath = query.from.path;
      const fromIsDirectory = isExistingDirectory(fromPath);
      workspaceRoot =
        explicitWorkspaceRoot ??
        (fromIsDirectory ? nodePath.resolve(fromPath) : undefined);
      uri = explicitUri ?? (fromIsDirectory ? undefined : fromPath);
    } else {
      uri = explicitUri ?? query.from.path;
    }
  } else if (query.from?.kind === 'materialized') {
    const scopePath = firstScopePath(query);
    const scopedUri = scopePath
      ? nodePath.isAbsolute(scopePath)
        ? scopePath
        : nodePath.join(query.from.localPath, scopePath)
      : undefined;
    if (isWorkspaceSymbol) {
      workspaceRoot = explicitWorkspaceRoot ?? query.from.localPath;
      uri = explicitUri ?? scopedUri;
    } else {
      uri = explicitUri ?? scopedUri ?? query.from.localPath;
    }
  } else if (query.from?.kind === 'github') {
    // remote semantics: materialize the file, then run LSP locally.
    const { owner, repo } = splitRepo(query.from);
    if (!owner || !repo) {
      diagnostics.push(
        diagnostic('invalidQuery', 'Remote semantics needs owner/repo.', {
          backend: 'lspGetSemantics',
        })
      );
      return { results: [], diagnostics, provenance };
    }
    const requestedUri =
      typeof semanticParams.uri === 'string' ? semanticParams.uri : undefined;
    const scopePath = firstScopePath(query);
    const sparsePath =
      requestedUri && !nodePath.isAbsolute(requestedUri)
        ? requestedUri
        : scopePath;
    const clone = await runDirect('ghCloneRepo', {
      owner,
      repo,
      ...(query.from.ref ? { branch: query.from.ref } : {}),
      ...(sparsePath ? { sparsePath } : {}),
    });
    const cloneData = firstQueryData<{ localPath?: string }>(clone).data;
    const cloneLocalPath = materializedClonePath(clone, cloneData?.localPath);
    if (!cloneLocalPath) {
      diagnostics.push(
        diagnostic(
          'materializationFailed',
          'Could not materialize repo for remote LSP.',
          { backend: 'ghCloneRepo' }
        )
      );
      return { results: [], diagnostics, provenance };
    }
    provenance.push({
      backend: 'ghCloneRepo',
      source: query.from,
      materializedPath: cloneLocalPath,
    });
    if (isWorkspaceSymbol) {
      workspaceRoot = explicitWorkspaceRoot ?? cloneLocalPath;
      if (requestedUri) {
        uri = nodePath.isAbsolute(requestedUri)
          ? requestedUri
          : nodePath.join(cloneLocalPath, requestedUri);
      } else if (scopePath) {
        uri = nodePath.isAbsolute(scopePath)
          ? scopePath
          : nodePath.join(cloneLocalPath, scopePath);
      }
    } else if (requestedUri) {
      uri = nodePath.isAbsolute(requestedUri)
        ? requestedUri
        : nodePath.join(cloneLocalPath, requestedUri);
    } else if (scopePath) {
      uri = nodePath.isAbsolute(scopePath)
        ? scopePath
        : nodePath.join(cloneLocalPath, scopePath);
    } else {
      uri = cloneLocalPath;
    }
  }

  if (!uri && !workspaceRoot) {
    diagnostics.push(
      diagnostic('invalidQuery', 'target:"semantics" needs a `from` anchor.', {
        backend: 'lspGetSemantics',
      })
    );
    return { results: [], diagnostics, provenance };
  }

  // params carry the LSP operation (type, symbolName, lineHint, …); for local
  // and materialized queries params.uri may override a directory/root `from`
  // anchor. For remote queries, params.uri has already been lowered to the
  // cloned sparse path above.
  const {
    uri: _ignoredUri,
    symbolKind,
    workspaceRoot: _ignoredWorkspaceRoot,
    ...lspParams
  } = semanticParams;
  const result = await runDirect('lspGetSemantics', {
    ...lspParams,
    ...(workspaceRoot ? { workspaceRoot } : {}),
    ...(uri ? { uri } : {}),
  });
  const { data, status } = firstQueryData(result);
  const recordData = data as Record<string, unknown> | undefined;
  const pagination = semanticPagination(recordData, query);
  const sourceUri = resolveSemanticSourceUri(
    stringFrom(recordData?.uri),
    uri,
    workspaceRoot
  );
  const source = semanticSource(query, sourceUri);
  const semanticItems = filterSemanticItemsByKind(
    expandSemanticData(recordData),
    symbolKind
  );
  return {
    results:
      status === 'error' ? [] : records(semanticItems, 'semantics', source),
    ...(pagination ? { pagination } : {}),
    diagnostics: [
      ...diagnostics,
      ...statusDiagnostics(result, 'lspGetSemantics'),
      ...semanticDiagnostics(recordData, query),
    ],
    provenance: [
      ...provenance,
      {
        backend: 'lspGetSemantics',
        source,
      },
    ],
  };
}

function expandSemanticData(
  data: Record<string, unknown> | undefined
): unknown[] {
  if (!data) return [];
  const payload = isRecord(data.payload) ? data.payload : undefined;
  const symbols = payload?.symbols;
  if (Array.isArray(symbols)) {
    const uri = stringFrom(data.uri);
    return symbols.map(symbol =>
      isRecord(symbol)
        ? {
            ...(uri && typeof symbol.uri !== 'string' ? { uri } : {}),
            ...symbol,
          }
        : symbol
    );
  }
  return expandData(data);
}

function filterSemanticItemsByKind(
  items: unknown[],
  symbolKind: unknown
): unknown[] {
  if (typeof symbolKind !== 'string' || !symbolKind.trim()) return items;
  const wanted = symbolKind.trim().toLowerCase();
  return items.filter(item => {
    if (!item || typeof item !== 'object') return false;
    const kind = (item as Record<string, unknown>).kind;
    return String(kind ?? '').toLowerCase() === wanted;
  });
}

function semanticSource(query: OqlQuery, uri: string): QuerySource {
  if (query.from?.kind === 'local') {
    return { ...query.from, path: uri };
  }
  if (query.from?.kind === 'materialized') {
    return { ...query.from, localPath: uri };
  }
  if (query.from?.kind === 'github') {
    return { kind: 'materialized', localPath: uri, source: query.from };
  }
  return query.from ?? { kind: 'local', path: uri };
}

export async function executeResearch(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;

  if (!root) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'target:"research" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlResearch',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"research" against the returned localPath.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  const facets = Array.isArray(p.facets)
    ? p.facets.filter((facet): facet is string => typeof facet === 'string')
    : undefined;
  const mode = requestedResearchMode(p.mode);
  if (mode === 'prove' && typeof p.intent !== 'string') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"research" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlResearch',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", facets:["symbols","files","relations"] }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  let data: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    data = await analyzeResearchFlow({
      root,
      goal: typeof p.goal === 'string' ? p.goal : undefined,
      intent: typeof p.intent === 'string' ? p.intent : undefined,
      facets,
      mode,
      maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
    });
  } catch (err) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          err instanceof Error
            ? err.message
            : 'Could not analyze the requested research root.',
          { backend: 'smartOqlResearch' }
        ),
      ],
      provenance: [{ backend: 'smartOqlResearch', source: query.from }],
    };
  }

  // Plan mode returns the flow only (no scan), so there is nothing to packetize.
  const { packets, graphSummary } =
    data.mode === 'plan'
      ? { packets: [], graphSummary: undefined }
      : buildResearchPackets(data);

  const caveats = [...data.caveats];
  if (p.mode === 'prove') {
    caveats.push(
      'mode:"prove" requested on target:"research": packets are candidate-grade unless LSP proof is attached. Native AST facts are included where available, but LSP reference proof is not run here. Use target:"graph" with proof:"lsp" or follow each packet\'s next.semantic.'
    );
  }
  const pageWindow = graphSummary
    ? packetPage(query, packets.length)
    : undefined;
  const pagedPackets = pageWindow
    ? packets.slice(pageWindow.packetsStart, pageWindow.packetsEnd)
    : [];
  if (
    pageWindow &&
    packets.length > 0 &&
    pageWindow.packetsStart >= packets.length
  ) {
    caveats.push(
      `Packet page ${pageWindow.pagination.currentPage} is outside the available packet range (${pageWindow.pagination.totalPages} page(s)).`
    );
  }

  // P1: detailed view returns per-domain *windows* (sliced + paged), not whole
  // arrays — honoring `select` so a narrow projection drops unrequested domains.
  const detailed =
    query.view === 'detailed'
      ? buildDetailedDomains(query, data)
      : { fields: {} as Record<string, unknown> };

  const enriched: Record<string, unknown> = {
    kind: data.kind,
    goal: data.goal,
    intent: data.intent,
    facets: data.facets,
    mode: data.mode,
    root: data.root,
    flow: data.flow,
    summary: data.summary,
    graphCapabilities: data.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(data.graphFacts),
    caveats,
    ...(graphSummary
      ? {
          graphSummary,
          packetPage: pageWindow?.pagination,
          packets: pagedPackets,
        }
      : {}),
    ...detailed.fields,
  };

  // The envelope pagination drives `next.page`; for detailed view it must
  // advance the packet window AND every detailed domain together.
  const pagination = combinePagination(
    pageWindow?.pagination,
    detailed.pagination
  );

  return {
    results: records([enriched], 'research', query.from),
    ...(pagination ? { pagination } : {}),
    diagnostics: [],
    provenance: [{ backend: 'smartOqlResearch', source: query.from }],
  };
}

export async function executeGraph(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const root =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;

  if (!root) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'target:"graph" needs a complete local file universe. Use a local/materialized source, or materialize a bounded GitHub corpus first.',
          {
            backend: 'smartOqlGraph',
            repair: {
              message:
                'Run target:"materialize" for a bounded GitHub repo/subtree, then run target:"graph" against the returned localPath.',
            },
          }
        ),
      ],
      provenance: [],
    };
  }

  const facets = Array.isArray(p.facets)
    ? p.facets.filter((facet): facet is string => typeof facet === 'string')
    : undefined;
  const mode = requestedResearchMode(p.mode);
  if (mode === 'prove' && typeof p.intent !== 'string') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"graph" mode:"prove" requires params.intent so the proof lane is deterministic. Use intent:"reachability"|"dependencies"|"symbols"|"general", then follow graph packet next.semantic/next.fetch continuations for missing proof.',
          {
            backend: 'smartOqlGraph',
            queryPath: 'params.intent',
            repair: {
              message:
                'Add params.intent. Example: params:{ mode:"prove", intent:"reachability", direction:"incoming" }.',
            },
          }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  let analysis: Awaited<ReturnType<typeof analyzeResearchFlow>>;
  try {
    analysis = await analyzeResearchFlow({
      root,
      goal: typeof p.goal === 'string' ? p.goal : undefined,
      intent: typeof p.intent === 'string' ? p.intent : undefined,
      facets,
      mode,
      maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
    });
  } catch (err) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          err instanceof Error
            ? err.message
            : 'Could not analyze the requested graph root.',
          { backend: 'smartOqlGraph' }
        ),
      ],
      provenance: [{ backend: 'smartOqlGraph', source: query.from }],
    };
  }

  const bundle =
    analysis.mode === 'plan' ? undefined : buildResearchPackets(analysis);
  const filters = graphFilters(p);
  const packets = bundle?.packets ?? [];
  const proofDiagnostics = shouldRunLspProof(analysis.mode, p)
    ? await escalateGraphPacketsWithLsp(
        root,
        query,
        packets,
        filters,
        graphProofLimit(query, p)
      )
    : [];
  const graphSummary = summarizePacketGraph(packets);
  const view = buildGraphView(
    query,
    packets,
    graphSummary,
    filters,
    analysis.graphFacts,
    root
  );

  const caveats = [
    ...(view.data.caveats ?? []),
    ...analysis.caveats,
    ...(analysis.mode === 'plan'
      ? ['mode:"plan" requested: graph packets were not built.']
      : []),
    ...(p.mode === 'prove'
      ? [
          shouldRunLspProof(analysis.mode, p)
            ? 'mode:"prove" requested: LSP proof escalation ran for the current graph page only. Follow next.page and next.semantic for remaining/open proof.'
            : 'mode:"prove" requested: graph rows are candidate-grade only. Follow packet next.semantic to confirm references.',
        ]
      : []),
  ];

  const enriched: OqlGraphData = {
    ...view.data,
    goal: analysis.goal,
    intent: analysis.intent,
    facets: analysis.facets,
    mode: analysis.mode,
    root: analysis.root,
    flow: analysis.flow,
    graphCapabilities: analysis.graphCapabilities,
    nativeGraphSummary: nativeGraphSummary(analysis.graphFacts),
    caveats,
  };

  return {
    results: records([enriched], 'graph', query.from),
    pagination: view.pagination,
    diagnostics: proofDiagnostics,
    provenance: [{ backend: 'smartOqlGraph', source: query.from }],
  };
}

/** Dispatch map: target -> adapter. */
export const RESEARCH_TARGET_ADAPTERS: Record<
  string,
  (q: OqlQuery) => Promise<AdapterResult>
> = {
  repositories: executeRepositories,
  packages: executePackages,
  pullRequests: executeHistory,
  commits: executeHistory,
  diff: executeDiff,
  artifacts: executeArtifacts,
  semantics: executeSemantics,
  research: executeResearch,
  graph: executeGraph,
};
