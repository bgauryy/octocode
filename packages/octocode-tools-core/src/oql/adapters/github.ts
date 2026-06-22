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
import type {
  GitHubFileContentData,
  GitHubSearchCodeGroup,
} from '@octocodeai/octocode-core/types';
import { runDirect } from './runner.js';
import { compileWhere } from './compile.js';
import { diagnostic } from '../diagnostics.js';
import type { AdapterResult } from './local.js';
import type {
  OqlCodeResultRow,
  OqlContentResultRow,
  OqlDiagnostic,
  OqlQuery,
  OqlTreeResultRow,
  QueryScope,
  QuerySource,
} from '../types.js';

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

function firstScopePath(scope: QueryScope | undefined): string | undefined {
  if (!scope?.path) return undefined;
  return Array.isArray(scope.path) ? scope.path[0] : scope.path;
}

function language(scope: QueryScope | undefined): string | undefined {
  const l = scope?.language;
  if (!l) return undefined;
  return Array.isArray(l) ? l[0] : l;
}

/** Pull the single query's `data` payload from a bulk CallToolResult. */
function extractData<T>(result: CallToolResult): T | undefined {
  const sc = result.structuredContent as
    | { results?: Array<{ data?: unknown }> }
    | undefined;
  const first = sc?.results?.[0];
  return first?.data as T | undefined;
}

function extractStatus(result: CallToolResult): string | undefined {
  const sc = result.structuredContent as
    | { results?: Array<{ status?: string }> }
    | undefined;
  return sc?.results?.[0]?.status;
}

/**
 * GitHub provider zero-results are NOT silent proof — code search can be
 * unindexed/deprecated and repo names redirect. Emit a non-blocking
 * `providerUnindexed` so an empty result reads as "verify", not "absent".
 */
function emptyProviderDiag(rowCount: number, backend: string): OqlDiagnostic[] {
  if (rowCount > 0) return [];
  return [
    diagnostic(
      'providerUnindexed',
      `${backend} returned no results — GitHub may not index this repo/branch (or the name redirected). Verify with structure/materialize before concluding absence.`,
      { backend, severity: 'info', blocksAnswer: false }
    ),
  ];
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
 * GitHub `files` lane: list files *containing* a positive text/regex predicate
 * via path-level code search (approximate). Predicates the provider cannot
 * enumerate by (field/structural/PCRE2/negation/boolean) are routed to
 * materialization by the planner and should not reach here; if one does, report
 * `requiresMaterialization` rather than silently returning nothing.
 */
async function githubFiles(query: OqlQuery): Promise<AdapterResult> {
  const where = query.where;
  const needsMat = (): AdapterResult => ({
    results: [],
    diagnostics: [
      diagnostic(
        'requiresMaterialization',
        'target:"files" over a GitHub source can only list files containing a term via the provider; everything else needs materialization (set materialize.mode:"auto" with a bounded scope.path, or use a local source).',
        { backend: 'localFindFiles' }
      ),
    ],
    provenance: [],
  });

  if (!where) return needsMat();
  const compiled = compileWhere(where);
  if (
    compiled.unsupported ||
    compiled.negate ||
    compiled.match?.mode === 'structural'
  ) {
    return needsMat();
  }

  const { owner, repo } = splitRepo(ghFrom(query));
  const p = query.params ?? {};
  const toolQuery: Record<string, unknown> = {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    keywords: [compiled.match?.keywords ?? ''],
    ...(language(query.scope) ? { language: language(query.scope) } : {}),
    ...(firstScopePath(query.scope)
      ? { path: firstScopePath(query.scope) }
      : {}),
    // `target:"files"` implies file-location intent — default to match:"path"
    // (cheapest) unless the agent explicitly set match via params.
    match: typeof p.match === 'string' ? p.match : 'path',
    ...(typeof p.concise === 'boolean' ? { concise: p.concise } : {}),
    ...(typeof p.extension === 'string' ? { extension: p.extension } : {}),
    ...(typeof p.filename === 'string' ? { filename: p.filename } : {}),
    ...(query.limit ? { limit: query.limit } : {}),
    ...(query.page ? { page: query.page } : {}),
  };
  const result = await runDirect('ghSearchCode', toolQuery);
  const data = extractData<{ results?: readonly GitHubSearchCodeGroup[] }>(
    result
  );
  // Distinct file paths — "files containing the term", not per-match rows.
  const seen = new Set<string>();
  const rows: import('../types.js').OqlFileResultRow[] = [];
  for (const group of data?.results ?? []) {
    for (const match of group.matches) {
      if (seen.has(match.path)) continue;
      seen.add(match.path);
      rows.push({
        kind: 'file',
        source: ghFrom(query),
        path: match.path,
        entryType: 'file',
      });
    }
  }
  return {
    results: rows,
    diagnostics: [
      ...statusDiagnostics(result, 'ghSearchCode'),
      ...emptyProviderDiag(rows.length, 'ghSearchCode'),
      ...(rows.length > 0
        ? [
            diagnostic(
              'providerSemanticsApproximate',
              'GitHub lists files containing a term at path level (index may be incomplete); materialize for an exact file set.',
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
  const where = query.where!;
  const compiled = compileWhere(where);
  if (compiled.unsupported || compiled.match?.mode === 'structural') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'requiresMaterialization',
          'This predicate cannot be evaluated by GitHub code search; materialize for local proof.',
          { backend: 'ghSearchCode' }
        ),
      ],
      provenance: [],
    };
  }

  const { owner, repo } = splitRepo(ghFrom(query));
  const p = query.params ?? {};
  const toolQuery: Record<string, unknown> = {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    keywords: [compiled.match?.keywords ?? ''],
    ...(language(query.scope) ? { language: language(query.scope) } : {}),
    ...(firstScopePath(query.scope)
      ? { path: firstScopePath(query.scope) }
      : {}),
    // Forward the smart `code`-target params (now typed in targetParams.ts):
    // match:"path" for cheap file-location, concise:true for flat results,
    // plus extension/filename filters the provider supports natively.
    ...(typeof p.match === 'string' ? { match: p.match } : {}),
    ...(typeof p.concise === 'boolean' ? { concise: p.concise } : {}),
    ...(typeof p.extension === 'string' ? { extension: p.extension } : {}),
    ...(typeof p.filename === 'string' ? { filename: p.filename } : {}),
    ...(query.limit ? { limit: query.limit } : {}),
    ...(query.page ? { page: query.page } : {}),
  };

  const result = await runDirect('ghSearchCode', toolQuery);
  const data = extractData<{ results?: readonly GitHubSearchCodeGroup[] }>(
    result
  );
  const rows: OqlCodeResultRow[] = [];
  for (const group of data?.results ?? []) {
    for (const match of group.matches) {
      // GitHub code search returns path-level matches with NO line — omit line
      // (do not fabricate); follow next.fetch for the exact location.
      rows.push({
        kind: 'code',
        source: ghFrom(query),
        path: match.path,
        ...(match.value !== undefined ? { snippet: match.value } : {}),
      });
    }
  }
  return {
    results: rows,
    diagnostics: [
      ...statusDiagnostics(result, 'ghSearchCode'),
      ...emptyProviderDiag(rows.length, 'ghSearchCode'),
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
  const toolQuery: Record<string, unknown> = {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    path: firstScopePath(query.scope) ?? '',
    minify,
    ...(ghFrom(query).kind === 'github' && ghFrom(query).ref
      ? { branch: ghFrom(query).ref }
      : {}),
    ...(c?.range?.startLine !== undefined
      ? { startLine: c.range.startLine }
      : {}),
    ...(c?.range?.endLine !== undefined ? { endLine: c.range.endLine } : {}),
    ...(c?.range?.contextLines !== undefined
      ? { contextLines: c.range.contextLines }
      : {}),
    ...(c?.match?.text !== undefined ? { matchString: c.match.text } : {}),
    ...(c?.match?.regex ? { matchStringIsRegex: true } : {}),
    ...(c?.match?.caseSensitive ? { matchStringCaseSensitive: true } : {}),
    ...(c?.charOffset !== undefined ? { charOffset: c.charOffset } : {}),
    ...(c?.charLength !== undefined ? { charLength: c.charLength } : {}),
    ...(c?.fullContent ? { fullContent: true } : {}),
  };
  const result = await runDirect('ghGetFileContent', toolQuery);
  const data = extractData<{
    results?: readonly GitHubFileContentData[];
    pagination?: {
      hasMore?: boolean;
      charOffset?: number;
      charLength?: number;
    };
  }>(result);
  // Report the requested view (the tool does not reliably echo the minify mode).
  const requestedView: OqlContentResultRow['contentView'] =
    minify === 'none' ? 'exact' : minify === 'symbols' ? 'symbols' : 'compact';
  const pag = data?.pagination;
  const hasCharWindow = typeof pag?.charOffset === 'number';
  const rows: OqlContentResultRow[] = (data?.results ?? []).map(d => {
    const range = {
      ...(d.startLine !== undefined ? { startLine: d.startLine } : {}),
      ...(d.endLine !== undefined ? { endLine: d.endLine } : {}),
      ...(hasCharWindow
        ? {
            charOffset: pag!.charOffset,
            ...(typeof pag!.charLength === 'number'
              ? { charLength: pag!.charLength }
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
  return {
    results: rows,
    ...(pag?.hasMore !== undefined
      ? { pagination: { hasMore: Boolean(pag.hasMore) } }
      : {}),
    diagnostics: [
      ...statusDiagnostics(result, 'ghGetFileContent'),
      ...emptyProviderDiag(rows.length, 'ghGetFileContent'),
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
  };
  const result = await runDirect('ghViewRepoStructure', toolQuery);
  const data = extractData<{
    structure?: Record<string, { files: string[]; folders: string[] }>;
  }>(result);
  const rows: OqlTreeResultRow[] = [];
  for (const [dir, entry] of Object.entries(data?.structure ?? {})) {
    for (const folder of entry.folders ?? []) {
      rows.push({
        kind: 'tree',
        source: ghFrom(query),
        path: `${dir}/${folder}`.replace(/\/+/g, '/'),
        entryType: 'directory',
        depth: 0,
      });
    }
    for (const file of entry.files ?? []) {
      rows.push({
        kind: 'tree',
        source: ghFrom(query),
        path: `${dir}/${file}`.replace(/\/+/g, '/'),
        entryType: 'file',
        depth: 0,
      });
    }
  }
  return {
    results: rows,
    diagnostics: [
      ...statusDiagnostics(result, 'ghViewRepoStructure'),
      ...emptyProviderDiag(rows.length, 'ghViewRepoStructure'),
    ],
    provenance: [{ backend: 'ghViewRepoStructure', source: ghFrom(query) }],
  };
}

function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const status = extractStatus(result);
  if (status === 'error') {
    const data = extractData<{ error?: string }>(result);
    return [
      diagnostic('invalidQuery', data?.error ?? 'GitHub backend error', {
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
