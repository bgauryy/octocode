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
  return items.map(item => ({
    kind: 'record',
    recordType,
    ...(source ? { source } : {}),
    data: (item && typeof item === 'object'
      ? (item as Record<string, unknown>)
      : { value: item }) as Record<string, unknown>,
  }));
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
  return {
    results: records(items, recordType, source),
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

export async function executeDiff(query: OqlQueryV1): Promise<AdapterResult> {
  // V2 diff reuses PR patch retrieval: params carry { prNumber, files? }.
  const { owner, repo } = splitRepo(query.from);
  const result = await runDirect('ghHistoryResearch', {
    ...(owner ? { owner } : {}),
    ...(repo ? { repo } : {}),
    content: { patches: { mode: 'all' } },
    ...params(query),
  });
  return finishRecords(
    result,
    'diff',
    'ghHistoryResearch',
    query.from ?? { kind: 'github' }
  );
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
  return finishRecords(result, 'artifact', 'localBinaryInspect', query.from);
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
};
