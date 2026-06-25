/**
 * V2 research-target adapters: semantics (LSP), repositories, packages,
 * pullRequests, commits, artifacts, diff.
 *
 * Each compiles a canonical OQL query (from + scope + `params` bag) into the
 * existing bulk tool runner and maps the single query's `data` payload into
 * generic record rows. Remote semantics route through materialization first
 * (clone → local LSP). This keeps the planner/dispatch uniform; per-target
 * specifics live behind one `params` bag validated by the backing tool.
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect } from './runner.js';
import { diagnostic } from '../diagnostics.js';
import { classifyDiffLane } from '../diffLanes.js';
import { analyzeResearchFlow } from '../research/analyze.js';
import type { AdapterResult } from './local.js';
import type {
  OqlDiagnostic,
  OqlQueryV1,
  OqlRecordResultRow,
  QuerySource,
} from '../types.js';

/* ------------------------------ helpers --------------------------------- */

function firstQueryData<T = Record<string, unknown>>(
  result: CallToolResult
): { data?: T; status?: string } {
  const sc = result.structuredContent as
    | { results?: Array<{ status?: string; data?: unknown }> }
    | undefined;
  const first = sc?.results?.[0];
  return { data: first?.data as T | undefined, status: first?.status };
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

/** Expand a tool `data` payload into row items (an inner array if present). */
function expandData(data: Record<string, unknown> | undefined): unknown[] {
  if (!data) return [];
  for (const key of RECORD_ARRAY_KEYS) {
    const v = (data as Record<string, unknown>)[key];
    if (Array.isArray(v)) return v;
  }
  return [data];
}

function records(
  items: unknown[],
  recordType: OqlRecordResultRow['recordType'],
  source?: QuerySource
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
      data,
    };
  });
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
        (s('owner') && s('repo') ? `${s('owner')}/${s('repo')}` : s('url'))
      );
    case 'package': {
      const name = s('name') ?? s('packageName');
      const ver = s('version');
      return name ? (ver ? `${name}@${ver}` : name) : undefined;
    }
    case 'pullRequest':
      return s('number') ? `#${s('number')}` : s('url');
    case 'commit':
      return s('sha')?.slice(0, 12) ?? s('oid')?.slice(0, 12);
    case 'artifact':
      return s('localPath') ?? s('path');
    case 'materialized':
      return s('localPath') ?? s('repoRoot');
    case 'diff':
      return s('path') ?? s('filename');
    case 'semantics': {
      const uri = s('uri');
      const line = s('line') ?? s('startLine');
      return uri ? (line ? `${uri}:${line}` : uri) : undefined;
    }
    case 'research':
      return s('intent') ?? s('goal') ?? 'research';
  }
}

function statusDiagnostics(
  result: CallToolResult,
  backend: string
): OqlDiagnostic[] {
  const { status, data } = firstQueryData<{ error?: string }>(result);
  if (status === 'error') {
    return [
      diagnostic('invalidQuery', data?.error ?? `${backend} failed`, {
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

function params(query: OqlQueryV1): Record<string, unknown> {
  return query.params ?? {};
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
  const pag = (
    data as {
      pagination?: {
        hasMore?: boolean;
        currentPage?: number;
        totalPages?: number;
      };
    }
  )?.pagination;
  const hasMore =
    pag?.hasMore === true ||
    Boolean((data as { next?: unknown })?.next) ||
    (typeof pag?.currentPage === 'number' &&
      typeof pag?.totalPages === 'number' &&
      pag.currentPage < pag.totalPages);
  return {
    results: records(items, recordType, source),
    ...(hasMore
      ? {
          pagination: {
            hasMore: true,
            ...(pag?.currentPage !== undefined
              ? { currentPage: pag.currentPage }
              : {}),
            ...(pag?.totalPages !== undefined
              ? { totalPages: pag.totalPages }
              : {}),
          },
        }
      : {}),
    diagnostics,
    provenance: [{ backend, source }],
  };
}

/* --------------------------- target adapters ---------------------------- */

export async function executeRepositories(
  query: OqlQueryV1
): Promise<AdapterResult> {
  const { owner } = splitRepo(query.from);
  const result = await runDirect('ghSearchRepos', {
    ...(owner ? { owner } : {}),
    ...params(query),
  });
  return finishRecords(
    result,
    'repository',
    'ghSearchRepos',
    query.from ?? { kind: 'github' }
  );
}

export async function executePackages(
  query: OqlQueryV1
): Promise<AdapterResult> {
  const result = await runDirect('npmSearch', { ...params(query) });
  return finishRecords(
    result,
    'package',
    'npmSearch',
    query.from ?? { kind: 'npm' }
  );
}

export async function executeHistory(
  query: OqlQueryV1
): Promise<AdapterResult> {
  const { owner, repo } = splitRepo(query.from);
  const commits = query.target === 'commits';
  const result = await runDirect('ghHistoryResearch', {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    ...(commits ? { type: 'commits' } : {}),
    ...params(query),
  });
  return finishRecords(
    result,
    commits ? 'commit' : 'pullRequest',
    'ghHistoryResearch',
    query.from ?? { kind: 'github' }
  );
}

/**
 * `target:"diff"` has two typed lanes, discriminated by params shape:
 *   - PR patch:    { prNumber, files? }            -> ghHistoryResearch patches
 *   - direct file: { baseRef, headRef, path }      -> two ghGetFileContent reads
 *                                                     + a pure local line diff
 * A request that fits neither returns a repair diagnostic rather than silently
 * falling through to a PR-patch call (see OCTOCODE_SEARCH_PARITY_CHECKLIST.md gap log #8).
 */
export async function executeDiff(query: OqlQueryV1): Promise<AdapterResult> {
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

/** Direct two-ref file diff via two content reads + a pure local line diff. */
async function executeDirectFileDiff(
  query: OqlQueryV1,
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
      filePath: refs.path,
      branch: ref,
      fullContent: true,
      minify: 'none',
    });

  const [baseRes, headRes] = await Promise.all([
    read(refs.baseRef),
    read(refs.headRef),
  ]);

  const base = firstQueryData<{ content?: string; error?: string }>(baseRes);
  const head = firstQueryData<{ content?: string; error?: string }>(headRes);

  if (base.status === 'error' || head.status === 'error') {
    const err =
      base.data?.error ?? head.data?.error ?? 'Could not read file at one ref.';
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', err, { backend: 'ghGetFileContent' }),
      ],
      provenance: [{ backend: 'ghGetFileContent', source: query.from }],
    };
  }

  const diff = computeLineDiff(
    base.data?.content ?? '',
    head.data?.content ?? ''
  );
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
  query: OqlQueryV1
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
    diagnostics,
    provenance: [{ backend: 'localBinaryInspect', source: query.from }],
  };
}

export async function executeSemantics(
  query: OqlQueryV1
): Promise<AdapterResult> {
  let uri: string | undefined;
  const provenance: AdapterResult['provenance'] = [];
  const diagnostics: OqlDiagnostic[] = [];

  if (query.from?.kind === 'local') {
    uri = query.from.path;
  } else if (query.from?.kind === 'materialized') {
    uri = query.from.localPath;
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
    const sparsePath =
      typeof (params(query) as { uri?: string }).uri === 'string'
        ? (params(query) as { uri: string }).uri
        : undefined;
    const clone = await runDirect('ghCloneRepo', {
      owner,
      repo,
      ...(query.from.ref ? { branch: query.from.ref } : {}),
      ...(sparsePath ? { sparsePath } : {}),
    });
    const cloneData = firstQueryData<{ localPath?: string }>(clone).data;
    if (!cloneData?.localPath) {
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
      materializedPath: cloneData.localPath,
    });
    uri =
      sparsePath && !sparsePath.startsWith('/')
        ? `${cloneData.localPath.replace(/\/$/, '')}/${sparsePath}`
        : cloneData.localPath;
  }

  if (!uri) {
    diagnostics.push(
      diagnostic('invalidQuery', 'target:"semantics" needs a `from` anchor.', {
        backend: 'lspGetSemantics',
      })
    );
    return { results: [], diagnostics, provenance };
  }

  // params carry the LSP operation (type, symbolName, lineHint, …); the
  // resolved absolute `uri` always wins over any params.uri used for cloning.
  const { uri: _ignoredUri, ...lspParams } = params(query) as {
    uri?: string;
  } & Record<string, unknown>;
  const result = await runDirect('lspGetSemantics', { ...lspParams, uri });
  const { data, status } = firstQueryData(result);
  return {
    results:
      status === 'error'
        ? []
        : records(expandData(data), 'semantics', query.from),
    diagnostics: [
      ...diagnostics,
      ...statusDiagnostics(result, 'lspGetSemantics'),
    ],
    provenance: [
      ...provenance,
      {
        backend: 'lspGetSemantics',
        source: query.from ?? { kind: 'local', path: uri },
      },
    ],
  };
}

export async function executeResearch(
  query: OqlQueryV1
): Promise<AdapterResult> {
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

  const data = await analyzeResearchFlow({
    root,
    goal: typeof p.goal === 'string' ? p.goal : undefined,
    intent: typeof p.intent === 'string' ? p.intent : undefined,
    facets: Array.isArray(p.facets)
      ? p.facets.filter((facet): facet is string => typeof facet === 'string')
      : undefined,
    mode: p.mode === 'plan' ? 'plan' : 'analyze',
    maxFiles: typeof p.maxFiles === 'number' ? p.maxFiles : undefined,
  });

  return {
    results: records([data], 'research', query.from),
    diagnostics: [],
    provenance: [{ backend: 'smartOqlResearch', source: query.from }],
  };
}

/** Dispatch map: V2 target -> adapter. */
export const V2_ADAPTERS: Record<
  string,
  (q: OqlQueryV1) => Promise<AdapterResult>
> = {
  repositories: executeRepositories,
  packages: executePackages,
  pullRequests: executeHistory,
  commits: executeHistory,
  diff: executeDiff,
  artifacts: executeArtifacts,
  semantics: executeSemantics,
  research: executeResearch,
};
