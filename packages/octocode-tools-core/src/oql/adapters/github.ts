/**
 * GitHub provider adapter: compile a canonical OQL query into the existing
 * GitHub tool runners and map their results. Provider-only lanes (text search,
 * content read, tree). Local-only predicates over GitHub are handled by the
 * materialization adapter, not here.
 *
 * The runners return an MCP `CallToolResult` whose `structuredContent.results`
 * carries one flattened entry per query (`{ id, status, data }`). We read the
 * single entry's `data`.
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { GitHubFileContentData } from '@octocodeai/octocode-core/types';
import { runDirect } from './runner.js';
import { toOqlPagination, type ToolPaginationPayload } from './pagination.js';
import { enrichCodePagination } from './resultMap.js';
import { diagnostic } from '../diagnostics.js';
import { toGithubCodeSearchToolQuery } from '../transformers/github/code.js';
import { firstScopePath } from '../transformers/github/common.js';
import type { AdapterResult } from './local.js';
import type {
  OqlCodeResultRow,
  OqlContentResultRow,
  OqlDiagnostic,
  OqlFileResultRow,
  OqlQuery,
  OqlTreeResultRow,
  QuerySource,
} from '../types.js';

type GitHubContentPagination = {
  currentPage?: number;
  totalPages?: number;
  hasMore?: boolean;
  charOffset?: number;
  charLength?: number;
  totalChars?: number;
};

type GitHubContentRow = GitHubFileContentData & {
  pagination?: GitHubContentPagination;
};

// NOTE: this adapter's splitRepo intentionally drops a bare (slash-less) repo on
// the else-branch, unlike transformers/github/common.splitGithubSource which
// keeps it — do not "dedupe" them without reconciling that difference first.
function splitRepo(source: QuerySource | undefined): {
  owner?: string;
  repo?: string;
} {
  if (source?.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

/** Pull the single query's `data` payload from a bulk CallToolResult. */
function extractData<T>(result: CallToolResult): T | undefined {
  const sc = result.structuredContent as
    | { results?: Array<{ data?: unknown } | Record<string, unknown>> }
    | undefined;
  const first = sc?.results?.[0];
  if (!first) return undefined;
  return ('data' in first ? first.data : first) as T | undefined;
}

function extractStatus(result: CallToolResult): string | undefined {
  const sc = result.structuredContent as
    { results?: Array<{ status?: string }> } | undefined;
  return sc?.results?.[0]?.status;
}

/**
 * GitHub provider zero-results are NOT silent proof — code search can be
 * unindexed/deprecated and repo names redirect. Emit a blocking diagnostic so
 * an empty provider read/search cannot be presented as complete proof.
 */
function emptyProviderDiag(rowCount: number, backend: string): OqlDiagnostic[] {
  if (rowCount > 0) return [];
  return [
    diagnostic(
      'providerUnindexed',
      `${backend} returned no results — GitHub may not index this repo/branch (or the name redirected). Do not treat this as absence: verify with \`search owner/repo[/path] --tree\`, then use bounded local proof via \`search <term> <path> --repo owner/repo --materialize required\`, \`clone owner/repo[/path]\`, or \`cache fetch owner/repo [path] --depth file|tree|clone\`.`,
      { backend, severity: 'warning', blocksAnswer: true }
    ),
  ];
}

interface GithubStructureEntry {
  dir?: string;
  files?: readonly string[];
  folders?: readonly string[];
}

interface GithubCodeSearchMatch {
  value?: string;
  matchIndices?: Array<{ start: number; end: number; lineOffset?: number }>;
}

interface GithubCodeSearchFile {
  owner?: string;
  repo?: string;
  queryId?: string;
  path: string;
  matches?: readonly GithubCodeSearchMatch[];
}

interface GithubCodeSearchPayload {
  files?: readonly (GithubCodeSearchFile | string)[];
  pagination?: ToolPaginationPayload;
}

function cleanRepoPath(part: string | undefined): string {
  if (!part || part === '.') return '';
  return part.replace(/^\/+|\/+$/g, '');
}

function joinRepoPath(...parts: Array<string | undefined>): string {
  return parts.map(cleanRepoPath).filter(Boolean).join('/');
}

function normalizeStructure(
  structure:
    | readonly GithubStructureEntry[]
    | Record<string, { files?: readonly string[]; folders?: readonly string[] }>
    | undefined
): GithubStructureEntry[] {
  if (!structure) return [];
  if (Array.isArray(structure)) return [...structure];
  return Object.entries(structure).map(([dir, entry]) => ({
    dir,
    files: entry.files,
    folders: entry.folders,
  }));
}

function structureDepth(pathValue: string): number {
  return cleanRepoPath(pathValue).split('/').filter(Boolean).length;
}

function normalizeExtension(value: string): string {
  return value.trim().toLowerCase().replace(/^\*\./, '').replace(/^\./, '');
}

function fileExtension(pathValue: string): string {
  const base = pathValue.split('/').pop() ?? pathValue;
  const dot = base.lastIndexOf('.');
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : '';
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function treePatternMatches(pathValue: string, pattern: string): boolean {
  const normalized = pattern.trim();
  if (!normalized) return true;
  const base = pathValue.split('/').pop() ?? pathValue;
  if (!normalized.includes('*') && !normalized.includes('?')) {
    return base.includes(normalized) || pathValue.includes(normalized);
  }
  const expression = normalized
    .split('**')
    .map(part =>
      part
        .split('*')
        .map(segment => segment.split('?').map(escapeRegex).join('[^/]'))
        .join('[^/]*')
    )
    .join('.*');
  const matcher = new RegExp(`^${expression}$`);
  return matcher.test(base) || matcher.test(pathValue);
}

function filterGithubTreeRows(
  rows: readonly OqlTreeResultRow[],
  query: OqlQuery
): OqlTreeResultRow[] {
  const tree = query.fetch?.tree;
  if (!tree) return [...rows];
  const extensions = (tree.extensions ?? [])
    .map(normalizeExtension)
    .filter(Boolean);

  return rows.filter(row => {
    if (tree.filesOnly && row.entryType !== 'file') return false;
    if (tree.directoriesOnly && row.entryType !== 'directory') return false;
    if (tree.pattern && !treePatternMatches(row.path, tree.pattern)) {
      return false;
    }
    return (
      row.entryType === 'directory' ||
      extensions.length === 0 ||
      extensions.includes(fileExtension(row.path))
    );
  });
}

function githubCodeFilePath(file: GithubCodeSearchFile | string): string {
  if (typeof file !== 'string') return file.path;
  const separator = file.indexOf(':');
  return separator >= 0 ? file.slice(separator + 1) : file;
}

function githubCodeFileMetadata(
  file: GithubCodeSearchFile | string
): Record<string, unknown> | undefined {
  if (typeof file === 'string') return undefined;
  const metadata = {
    ...(file.owner !== undefined ? { owner: file.owner } : {}),
    ...(file.repo !== undefined ? { repo: file.repo } : {}),
    ...(file.queryId !== undefined ? { queryId: file.queryId } : {}),
  };
  return Object.keys(metadata).length ? metadata : undefined;
}

function githubCodeFileMatches(
  file: GithubCodeSearchFile | string
): readonly GithubCodeSearchMatch[] {
  return typeof file === 'string' ? [] : (file.matches ?? []);
}

export async function executeGithub(query: OqlQuery): Promise<AdapterResult> {
  switch (query.target) {
    case 'content':
      return githubContent(query);
    case 'structure':
      return githubStructure(query);
    case 'files':
      return githubFiles(query);
    case 'code':
    default:
      return githubCode(query);
  }
}

/**
 * GitHub `files` lane: list files via path-level code search. Positive
 * text/regex predicates list files *containing* a term (approximate); path-like
 * field equality (`basename`/`extension`/`path` "=") lists files by provider
 * path qualifier (the same route as OQL target:"files" path discovery). Predicates the
 * provider cannot enumerate by (other field ops, structural/PCRE2, negation,
 * boolean) are routed to materialization by the planner and should not reach
 * here; if one does, report `requiresMaterialization` rather than silently
 * returning nothing.
 */
async function githubFiles(query: OqlQuery): Promise<AdapterResult> {
  const transformed = toGithubCodeSearchToolQuery(query, {
    defaultMatch: 'file',
    unsupportedBackend: 'localFindFiles',
    unsupportedMessage:
      'target:"files" over a GitHub source can only list files containing a term via the provider; everything else needs materialization (set materialize.mode:"auto" with a bounded scope.path, or use a local source).',
  });
  if (!transformed.ok) {
    return {
      results: [],
      diagnostics: transformed.diagnostics,
      provenance: [],
    };
  }

  const result = await runDirect('ghSearchCode', transformed.query);
  const data = extractData<GithubCodeSearchPayload>(result);
  // Distinct file paths — "files containing the term", not per-match rows.
  const seen = new Set<string>();
  const rows: OqlFileResultRow[] = [];
  for (const file of data?.files ?? []) {
    const path = githubCodeFilePath(file);
    if (seen.has(path)) continue;
    seen.add(path);
    rows.push({
      kind: 'file',
      source: ghFrom(query),
      path,
      entryType: 'file',
    });
  }
  const pagination = toOqlPagination(data?.pagination);
  return {
    results: rows,
    ...(pagination ? { pagination } : {}),
    diagnostics: [
      ...providerDiagnostics(result, rows.length, 'ghSearchCode'),
      ...(rows.length > 0
        ? [
            diagnostic(
              'providerSemanticsApproximate',
              'GitHub lists files containing a term via provider code search (index may be incomplete); materialize for an exact file set.',
              { backend: 'ghSearchCode', severity: 'info', blocksAnswer: false }
            ),
          ]
        : []),
    ],
    provenance: [{ backend: 'ghSearchCode', source: ghFrom(query) }],
  };
}

/** GitHub source, guaranteed by dispatch. */
type GithubSource = Extract<QuerySource, { kind: 'github' }>;
function ghFrom(query: OqlQuery): GithubSource {
  return (query.from ?? { kind: 'github' }) as GithubSource;
}

async function githubCode(query: OqlQuery): Promise<AdapterResult> {
  const transformed = toGithubCodeSearchToolQuery(query);
  if (!transformed.ok) {
    return {
      results: [],
      diagnostics: transformed.diagnostics,
      provenance: [],
    };
  }

  const result = await runDirect('ghSearchCode', transformed.query);
  const data = extractData<GithubCodeSearchPayload>(result);
  const rows: OqlCodeResultRow[] = [];
  for (const file of data?.files ?? []) {
    const path = githubCodeFilePath(file);
    const metadata = githubCodeFileMetadata(file);
    const matches = githubCodeFileMatches(file);
    if (matches.length === 0) {
      rows.push({
        kind: 'code',
        source: ghFrom(query),
        path,
        ...(metadata ? { metadata } : {}),
      });
      continue;
    }
    for (const match of matches) {
      // GitHub code search returns path-level matches with NO line — omit line
      // (do not fabricate); follow next.fetch for the exact location.
      rows.push({
        kind: 'code',
        source: ghFrom(query),
        path,
        ...(match.value !== undefined ? { snippet: match.value } : {}),
        ...(match.matchIndices !== undefined
          ? { matchIndices: match.matchIndices }
          : {}),
        ...(metadata ? { metadata } : {}),
      });
    }
  }
  // GitHub code search paginates FILE items while OQL code rows are per-match:
  // mark the unit (itemUnit/rowCount) so the runner defers to backend file
  // paging instead of slicing match rows out from under `next.page`. Do NOT
  // overwrite totalItemsKind — it carries GitHub's lowerBound/reported
  // exactness for the total.
  const pagination = enrichCodePagination(
    toOqlPagination(data?.pagination),
    rows.length,
    true
  );
  return {
    results: rows,
    ...(pagination ? { pagination } : {}),
    diagnostics: [
      ...providerDiagnostics(result, rows.length, 'ghSearchCode'),
      // GitHub code search is path-level and (for regex) approximate; the
      // planner/capabilities layer owns the providerSemanticsApproximate
      // diagnostic, so the adapter only notes the missing line locality.
      ...(rows.length > 0
        ? [
            diagnostic(
              'providerSemanticsApproximate',
              'GitHub code search returns path-level hits without line numbers; follow next.fetch for exact location/lines.',
              { backend: 'ghSearchCode', severity: 'info', blocksAnswer: false }
            ),
          ]
        : []),
    ],
    provenance: [{ backend: 'ghSearchCode', source: ghFrom(query) }],
  };
}

async function githubContent(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(ghFrom(query));
  const c = query.fetch?.content;
  const minify =
    c?.contentView === 'exact'
      ? 'none'
      : c?.contentView === 'symbols'
        ? 'symbols'
        : 'standard';
  const range = normalizeContentRange(c?.range);
  const toolQuery: Record<string, unknown> = {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    path: firstScopePath(query.scope) ?? '',
    type: 'file',
    minify,
    ...(ghFrom(query).kind === 'github' && ghFrom(query).ref
      ? { branch: ghFrom(query).ref }
      : {}),
    ...range,
    ...(c?.match?.text !== undefined ? { matchString: c.match.text } : {}),
    ...(c?.match?.regex ? { matchStringIsRegex: true } : {}),
    ...(c?.match?.caseSensitive ? { matchStringCaseSensitive: true } : {}),
    ...(c?.charOffset !== undefined ? { charOffset: c.charOffset } : {}),
    ...(c?.charLength !== undefined ? { charLength: c.charLength } : {}),
    ...(c?.fullContent ? { fullContent: true } : {}),
  };
  const result = await runDirect('ghGetFileContent', toolQuery);
  const data = extractData<{
    results?: readonly GitHubContentRow[];
    files?: readonly GitHubContentRow[];
    pagination?: GitHubContentPagination;
  }>(result);
  // Report the requested view (the tool does not reliably echo the minify mode).
  const requestedView: OqlContentResultRow['contentView'] =
    minify === 'none' ? 'exact' : minify === 'symbols' ? 'symbols' : 'compact';
  const pag = data?.pagination;
  const fileRows = data?.results ?? data?.files ?? [];
  const rows: OqlContentResultRow[] = fileRows.map(d => {
    const rowPagination = d.pagination ?? pag;
    const hasCharWindow = typeof rowPagination?.charOffset === 'number';
    const range = {
      ...(d.startLine !== undefined ? { startLine: d.startLine } : {}),
      ...(d.endLine !== undefined ? { endLine: d.endLine } : {}),
      ...(hasCharWindow
        ? {
            charOffset: rowPagination!.charOffset,
            ...(typeof rowPagination!.charLength === 'number'
              ? { charLength: rowPagination!.charLength }
              : {}),
          }
        : {}),
    };
    return {
      kind: 'content' as const,
      source: ghFrom(query),
      path: d.path,
      content: d.content,
      contentView: requestedView,
      ...(Object.keys(range).length ? { range } : {}),
    };
  });
  const pagination = pag ?? fileRows.find(d => d.pagination)?.pagination;
  return {
    results: rows,
    ...(pagination?.hasMore !== undefined
      ? {
          pagination: {
            hasMore: Boolean(pagination.hasMore),
            ...(pagination.currentPage !== undefined
              ? { currentPage: pagination.currentPage }
              : {}),
            ...(pagination.totalPages !== undefined
              ? { totalPages: pagination.totalPages }
              : {}),
            ...(pagination.charLength !== undefined
              ? { itemsPerPage: pagination.charLength }
              : {}),
            ...(pagination.totalChars !== undefined
              ? {
                  totalItems: pagination.totalChars,
                  totalItemsKind: 'chars',
                }
              : {}),
          },
        }
      : {}),
    diagnostics: [
      ...providerDiagnostics(result, rows.length, 'ghGetFileContent'),
    ],
    provenance: [{ backend: 'ghGetFileContent', source: ghFrom(query) }],
  };
}

async function githubStructure(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(ghFrom(query));
  const toolQuery: Record<string, unknown> = {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    path: firstScopePath(query.scope) ?? '',
    ...(ghFrom(query).kind === 'github' && ghFrom(query).ref
      ? { branch: ghFrom(query).ref }
      : {}),
    ...(query.fetch?.tree?.maxDepth !== undefined
      ? { maxDepth: query.fetch.tree.maxDepth }
      : {}),
    ...(query.fetch?.tree?.includeSizes ? { includeSizes: true } : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
  };
  const result = await runDirect('ghViewRepoStructure', toolQuery);
  const data = extractData<{
    structure?:
      | readonly GithubStructureEntry[]
      | Record<
          string,
          { files?: readonly string[]; folders?: readonly string[] }
        >;
    pagination?: ToolPaginationPayload;
  }>(result);
  const rows: OqlTreeResultRow[] = [];
  const scopePath = firstScopePath(query.scope);
  for (const entry of normalizeStructure(data?.structure)) {
    const dir = entry.dir ?? '.';
    for (const folder of entry.folders ?? []) {
      const pathValue = joinRepoPath(scopePath, dir, folder);
      rows.push({
        kind: 'tree',
        source: ghFrom(query),
        path: pathValue,
        entryType: 'directory',
        depth: structureDepth(pathValue),
      });
    }
    for (const file of entry.files ?? []) {
      const pathValue = joinRepoPath(scopePath, dir, file);
      rows.push({
        kind: 'tree',
        source: ghFrom(query),
        path: pathValue,
        entryType: 'file',
        depth: structureDepth(pathValue),
      });
    }
  }
  const filteredRows = filterGithubTreeRows(rows, query);
  const pagination = toOqlPagination(data?.pagination);
  return {
    results:
      query.limit !== undefined
        ? filteredRows.slice(0, query.limit)
        : filteredRows,
    ...(pagination ? { pagination } : {}),
    diagnostics: [
      ...providerDiagnostics(
        result,
        filteredRows.length,
        'ghViewRepoStructure'
      ),
    ],
    provenance: [{ backend: 'ghViewRepoStructure', source: ghFrom(query) }],
  };
}

function normalizeContentRange(
  range: NonNullable<NonNullable<OqlQuery['fetch']>['content']>['range']
): { startLine?: number; endLine?: number } {
  if (range?.startLine === undefined) return {};
  const contextLines = range.contextLines ?? 0;
  const startLine = Math.max(1, range.startLine - contextLines);
  const endLine = (range.endLine ?? range.startLine) + contextLines;
  return { startLine, endLine };
}

type ProviderErrorInfo = {
  message: string;
  status?: number;
  retryAfterSeconds?: number;
  rateLimitRemaining?: number;
};

function finiteNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

/**
 * Read a provider failure from BOTH result shapes:
 * - default bulk path: `results[0].status === 'error'` with `data.error`
 *   carrying the structured provider error (or a plain string);
 * - finalized tools (ghSearchCode/ghGetFileContent): errored results are
 *   stripped into top-level `errors[]`. OQL sends exactly one query per
 *   runDirect call, so `errors[0]` is that query's failure — without this
 *   read, a 403/429 used to fire no status diagnostic at all and surfaced
 *   as the misleading providerUnindexed.
 */
function providerErrorInfo(
  result: CallToolResult
): ProviderErrorInfo | undefined {
  if (extractStatus(result) === 'error') {
    const err = extractData<{ error?: unknown }>(result)?.error;
    if (typeof err === 'string' && err) return { message: err };
    if (err && typeof err === 'object') {
      const o = err as Record<string, unknown>;
      return {
        message:
          typeof o.error === 'string' && o.error
            ? o.error
            : 'GitHub backend error',
        status: finiteNumber(o.status),
        retryAfterSeconds: finiteNumber(o.retryAfter),
        rateLimitRemaining: finiteNumber(o.rateLimitRemaining),
      };
    }
    return { message: 'GitHub backend error' };
  }
  const sc = result.structuredContent as
    | {
        errors?: Array<{
          error?: unknown;
          status?: unknown;
          retryAfterSeconds?: unknown;
          rateLimitRemaining?: unknown;
        }>;
      }
    | undefined;
  const e = sc?.errors?.[0];
  if (!e) return undefined;
  return {
    message:
      typeof e.error === 'string' && e.error ? e.error : 'GitHub backend error',
    status: finiteNumber(e.status),
    retryAfterSeconds: finiteNumber(e.retryAfterSeconds),
    rateLimitRemaining: finiteNumber(e.rateLimitRemaining),
  };
}

function classifyProviderError(
  info: ProviderErrorInfo,
  backend: string
): OqlDiagnostic {
  const rateLimitLike = /rate limit|secondary rate/i.test(info.message);
  const authLike =
    /bad credentials|requires authentication|saml|not accessible by/i.test(
      info.message
    );
  if (
    info.status === 429 ||
    (info.status === 403 &&
      (info.rateLimitRemaining === 0 ||
        info.retryAfterSeconds !== undefined ||
        rateLimitLike)) ||
    (info.status === undefined && rateLimitLike)
  ) {
    const wait =
      info.retryAfterSeconds !== undefined
        ? ` Retry after ~${info.retryAfterSeconds}s.`
        : '';
    // Transient → warning severity, but still blocks proof via BLOCKING_CODES:
    // a rate-limited call evaluated nothing.
    return diagnostic('rateLimited', `${info.message}${wait}`, {
      backend,
      severity: 'warning',
      repair: {
        message:
          'Wait for the rate-limit window to reset and re-run the same query, or authenticate (OCTOCODE_TOKEN/GH_TOKEN/GITHUB_TOKEN) to raise limits.',
      },
    });
  }
  if (
    info.status === 401 ||
    ((info.status === 403 || info.status === undefined) && authLike)
  ) {
    return diagnostic('authRequired', info.message, {
      backend,
      repair: {
        message:
          'Provide a valid token (OCTOCODE_TOKEN/GH_TOKEN/GITHUB_TOKEN) with access to this repo, then re-run the same query.',
      },
    });
  }
  return diagnostic('invalidQuery', info.message, { backend });
}

function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const info = providerErrorInfo(result);
  if (info) return [classifyProviderError(info, backend)];
  if (extractStatus(result) === 'empty') {
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
 * Status + gated empty-provider diagnostics. A provider failure (rate limit,
 * auth, invalid query) already explains the empty result — emitting
 * providerUnindexed on top would misread a 403/429 as "repo not indexed".
 */
function providerDiagnostics(
  result: CallToolResult,
  rowCount: number,
  backend: string
): OqlDiagnostic[] {
  const status = statusDiagnostics(result, backend);
  if (status.some(d => d.severity !== 'info')) return status;
  return [...status, ...emptyProviderDiag(rowCount, backend)];
}
