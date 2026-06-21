/**
 * Materialization (remote-as-local) adapter.
 *
 * Bounded GitHub repo/subtree -> local clone -> run the local adapter for exact
 * proof (structural AST, PCRE2, exact content). Bounds are enforced: a broad
 * scope or unbounded full-repo clone is refused at planning time; this adapter
 * additionally maps `scope.path` to a sparse checkout.
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect } from './runner.js';
import { executeLocal, type AdapterResult } from './local.js';
import { diagnostic } from '../diagnostics.js';
import type { OqlQueryV1, QueryScope, QuerySource } from '../types.js';

function splitRepo(source: QuerySource): { owner?: string; repo?: string } {
  if (source.kind !== 'github') return {};
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

function extractClone(result: CallToolResult): {
  localPath?: string;
  cached?: boolean;
  error?: string;
  status?: string;
} {
  const sc = result.structuredContent as
    | { results?: Array<{ status?: string; data?: Record<string, unknown> }> }
    | undefined;
  const first = sc?.results?.[0];
  const data = first?.data as
    | { localPath?: string; cached?: boolean; error?: string }
    | undefined;
  return {
    localPath: data?.localPath,
    cached: data?.cached,
    error: data?.error,
    status: first?.status,
  };
}

export async function executeMaterialize(
  query: OqlQueryV1
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
  const localQuery: OqlQueryV1 = {
    ...query,
    from: { kind: 'materialized', localPath, source: from },
    ...(query.scope ? { scope: { ...query.scope, path: undefined } } : {}),
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
