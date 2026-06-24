/**
 * Materialization (remote-as-local) adapter.
 *
 * Bounded GitHub repo/subtree -> local clone -> run the local adapter for exact
 * proof (structural AST, PCRE2, exact content). Bounds are enforced: a broad
 * scope or unbounded full-repo clone is refused at planning time; this adapter
 * additionally maps `scope.path` to a sparse checkout.
 */
import path from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect } from './runner.js';
import { executeLocal, type AdapterResult } from './local.js';
import { diagnostic } from '../diagnostics.js';
import { firstScopePath } from '../transformers/github/common.js';
import type { OqlQuery, OqlRecordResultRow, QuerySource } from '../types.js';

function dedupe(values: string[]): string[] {
  return [...new Set(values)];
}

// See adapters/github.ts: splitRepo intentionally differs from
// common.splitGithubSource on the slash-less repo case.
function splitRepo(source: QuerySource): { owner?: string; repo?: string } {
  if (source.kind !== 'github') return {};
  if (source.repo && source.repo.includes('/')) {
    const [owner, repo] = source.repo.split('/');
    return { owner, repo };
  }
  return { owner: source.owner };
}

function extractClone(result: CallToolResult): {
  localPath?: string;
  cached?: boolean;
  error?: string;
  status?: string;
} {
  const sc = result.structuredContent as
    | {
        base?: string;
        results?: Array<{ status?: string; data?: Record<string, unknown> }>;
      }
    | undefined;
  const first = sc?.results?.[0];
  const data = first?.data as
    | { localPath?: string; cached?: boolean; error?: string }
    | undefined;
  const localPath =
    data?.localPath && sc?.base && !path.isAbsolute(data.localPath)
      ? path.join(sc.base, data.localPath)
      : data?.localPath;
  return {
    localPath,
    cached: data?.cached,
    error: data?.error,
    status: first?.status,
  };
}

export async function executeMaterialize(
  query: OqlQuery
): Promise<AdapterResult> {
  if (query.from?.kind !== 'github') {
    // already local/materialized — no clone needed
    return executeLocal(query);
  }
  const from = query.from;

  const { owner, repo } = splitRepo(from);
  if (!owner || !repo) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'materializationFailed',
          'Materialization requires a concrete owner/repo.',
          { backend: 'ghCloneRepo' }
        ),
      ],
      provenance: [],
    };
  }

  const sparsePath = firstScopePath(query.scope);
  const cloneQuery: Record<string, unknown> = {
    owner,
    repo,
    ...(from.ref ? { branch: from.ref } : {}),
    ...(query.materialize?.strategy !== 'repo' && sparsePath
      ? { sparsePath }
      : {}),
    ...(query.materialize?.forceRefresh ? { forceRefresh: true } : {}),
  };

  const cloneResult = await runDirect('ghCloneRepo', cloneQuery);
  const { localPath, cached, error, status } = extractClone(cloneResult);

  if (status === 'error' || !localPath) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'materializationFailed',
          error ?? 'Clone/fetch failed; cannot run local proof.',
          { backend: 'ghCloneRepo' }
        ),
      ],
      provenance: [{ backend: 'ghCloneRepo', source: from }],
    };
  }

  // Re-root the query at the materialized path. scope.path already became the
  // sparse checkout root, so drop it from the local scope to avoid double-join.
  // Clone byproducts (.git internals, the .octocode-clone-meta.json marker) are
  // not part of the repo's file set — exclude them so materialized listings and
  // totals match a real checkout (audit #11).
  const baseScope = query.scope ?? {};
  const localQuery: OqlQuery = {
    ...query,
    from: { kind: 'materialized', localPath, source: from },
    scope: {
      ...baseScope,
      path: undefined,
      excludeDir: dedupe([...(baseScope.excludeDir ?? []), '.git']),
      exclude: dedupe([
        ...(baseScope.exclude ?? []),
        '.octocode-clone-meta.json',
        '**/.octocode-clone-meta.json',
      ]),
    },
  };

  const localResult = await executeLocal(localQuery);
  return {
    ...localResult,
    diagnostics: [
      ...localResult.diagnostics,
      ...(cached
        ? [
            diagnostic(
              'staleCache',
              'Result came from a cached clone; set materialize.forceRefresh to refresh.',
              { backend: 'ghCloneRepo', severity: 'info', blocksAnswer: false }
            ),
          ]
        : []),
    ],
    provenance: [
      {
        backend: 'ghCloneRepo',
        source: from,
        materializedPath: localPath,
        cache: cached ? 'hit' : 'miss',
      },
      ...localResult.provenance,
    ],
  };
}

/**
 * `target:"materialize"` — addressable materialization. Clone/cache a bounded
 * corpus once and return a stable local checkpoint row (localPath, repoRoot,
 * source, ref, cache, complete) that downstream queries can root at via the
 * `next.search` / `next.structure` / `next.fetch` continuations (attached in
 * run.ts). This makes materialization a first-class step, not a search side-effect.
 */
export async function executeMaterializeCheckpoint(
  query: OqlQuery
): Promise<AdapterResult> {
  // Already materialized: echo the existing checkpoint (no re-clone).
  if (query.from?.kind === 'materialized') {
    const localPath = query.from.localPath;
    return {
      results: [checkpointRow(localPath, query.from.source, undefined, true)],
      diagnostics: [],
      provenance: [{ backend: 'ghCloneRepo', source: query.from }],
    };
  }

  if (query.from?.kind !== 'github') {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"materialize" needs a GitHub source (owner/repo) or an already-materialized `from`.',
          { backend: 'ghCloneRepo' }
        ),
      ],
      provenance: [],
    };
  }

  const from = query.from;
  const { owner, repo } = splitRepo(from);
  if (!owner || !repo) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'materializationFailed',
          'Materialization requires a concrete owner/repo.',
          { backend: 'ghCloneRepo' }
        ),
      ],
      provenance: [],
    };
  }

  const fullRepo = query.materialize?.strategy === 'repo';
  const sparsePath = fullRepo ? undefined : firstScopePath(query.scope);
  const cloneQuery: Record<string, unknown> = {
    owner,
    repo,
    ...(from.ref ? { branch: from.ref } : {}),
    ...(sparsePath ? { sparsePath } : {}),
    ...(query.materialize?.forceRefresh ? { forceRefresh: true } : {}),
  };

  const cloneResult = await runDirect('ghCloneRepo', cloneQuery);
  const { localPath, cached, error, status } = extractClone(cloneResult);

  if (status === 'error' || !localPath) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'materializationFailed',
          error ?? 'Clone/fetch failed; no checkpoint produced.',
          { backend: 'ghCloneRepo' }
        ),
      ],
      provenance: [{ backend: 'ghCloneRepo', source: from }],
    };
  }

  // `complete` = the whole corpus is local (full-repo clone); a bounded sparse
  // subtree is materialized-but-partial.
  const complete = !sparsePath;
  return {
    results: [checkpointRow(localPath, from, from.ref, complete, cached)],
    diagnostics: cached
      ? [
          diagnostic(
            'staleCache',
            'Checkpoint served from a cached clone; set materialize.forceRefresh to refresh.',
            { backend: 'ghCloneRepo', severity: 'info', blocksAnswer: false }
          ),
        ]
      : [],
    provenance: [
      {
        backend: 'ghCloneRepo',
        source: from,
        materializedPath: localPath,
        cache: cached ? 'hit' : 'miss',
      },
    ],
  };
}

function checkpointRow(
  localPath: string,
  source: QuerySource | undefined,
  ref: string | undefined,
  complete: boolean,
  cached?: boolean
): OqlRecordResultRow {
  return {
    kind: 'record',
    recordType: 'materialized',
    id: localPath,
    ...(source ? { source } : {}),
    data: {
      localPath,
      repoRoot: localPath,
      ...(source ? { sourceRef: source } : {}),
      ...(ref ? { ref } : {}),
      cache: cached ? 'hit' : 'miss',
      complete,
    },
  };
}
