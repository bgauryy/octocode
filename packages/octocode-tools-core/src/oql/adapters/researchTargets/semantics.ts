/**
 * `target:"semantics"` adapter: LSP semantics for local/materialized/github
 * sources. Remote sources are materialized first (clone → local LSP); the
 * anchor resolution and empty/partial diagnostics live here.
 */
import nodePath from 'node:path';
import { runDirect, firstQueryData, stringFrom } from '../runner.js';
import { diagnostic } from '../../diagnostics.js';
import { firstScopePath } from '../../transformers/github/common.js';
import type { AdapterResult } from '../local.js';
import { expandData, isRecord, records } from './rows.js';
import {
  semanticPageContinuation,
  semanticPagination,
  statusDiagnostics,
} from './pagination.js';
import {
  isExistingDirectory,
  materializedClonePath,
  params,
  splitRepo,
} from './shared.js';
import type { OqlDiagnostic, OqlQuery, QuerySource } from '../../types.js';

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
    const scopePath = firstScopePath(query.scope);
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
    const scopePath = firstScopePath(query.scope);
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
