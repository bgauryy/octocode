/**
 * GitHub provider adapter — per-lane execute functions. Each function compiles
 * a canonical OQL query into the existing GitHub tool runners and maps their
 * results back into OQL result rows.
 *
 * The runners return an MCP `CallToolResult` whose `structuredContent.results`
 * carries one flattened entry per query (`{ id, status, data }`). We read the
 * single entry's `data`.
 */
import { runDirect } from '../runner.js';
import { toOqlPagination, type ToolPaginationPayload } from '../pagination.js';
import { enrichCodePagination } from '../resultMap.js';
import { diagnostic } from '../../diagnostics.js';
import { toGithubCodeSearchToolQuery } from '../../transformers/github/code.js';
import { firstScopePath } from '../../transformers/github/common.js';
import type { AdapterResult } from '../local.js';
import type {
  OqlCodeResultRow,
  OqlContentResultRow,
  OqlFileResultRow,
  OqlQuery,
  OqlTreeResultRow,
} from '../../types.js';
import { providerDiagnostics } from './provider-diagnostics.js';
import {
  extractData,
  filterGithubTreeRows,
  ghFrom,
  githubCodeFileMatches,
  githubCodeFileMetadata,
  githubCodeFilePath,
  joinRepoPath,
  normalizeContentRange,
  normalizeStructure,
  splitRepo,
  structureDepth,
  type GitHubContentPagination,
  type GitHubContentRow,
  type GithubCodeSearchPayload,
  type GithubStructureEntry,
} from './shared.js';

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
export async function githubFiles(query: OqlQuery): Promise<AdapterResult> {
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

export async function githubCode(query: OqlQuery): Promise<AdapterResult> {
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

export async function githubContent(query: OqlQuery): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(ghFrom(query));
  const c = query.fetch?.content;
  // contentView and minify now share one vocabulary (none/standard/symbols),
  // so this is a direct passthrough rather than a translation.
  const minify = c?.contentView ?? 'standard';
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
  const requestedView: OqlContentResultRow['contentView'] = minify;
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

export async function githubStructure(query: OqlQuery): Promise<AdapterResult> {
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
