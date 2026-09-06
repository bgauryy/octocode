import {
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
} from '../shared/semanticTypes.js';
import { buildNextPageContinuation } from '../../../scheme/pagination.js';
import { MAX_PAGE_NUMBER } from '../../../config.js';
import { isRecord, isSemanticEnvelope } from './semanticPresentation.js';

// Empty-state categories that warrant a ready-to-run fallback. Anything that
// left the agent without a location (symbol never resolved, operation
// unsupported, or a genuinely empty result set) should point somewhere useful.
const FALLBACK_EMPTY_CATEGORIES: ReadonlySet<string> = new Set([
  'unsupportedOperation',
  'symbolNotFound',
  'anchorFailed',
  'noLocations',
  'noReferences',
  'noHover',
  'noCalls',
  'noWorkspaceSymbols',
  'noTypeHierarchy',
  'noDiagnostics',
]);

// Categories where the anchor itself may be wrong (stale symbol / bad lineHint),
// so re-anchoring — not just a text search — is the right next move.
const REANCHOR_EMPTY_CATEGORIES: ReadonlySet<string> = new Set([
  'symbolNotFound',
  'anchorFailed',
]);

// Reads the empty-state marker from any payload shape: the dedicated `empty`
// kind carries category/reason at the top level, other kinds nest it under
// `empty` (SemanticEmptyState).
function emptyStateOf(
  payload: Record<string, unknown>
): { category: string; reason?: string } | undefined {
  if (payload.kind === 'empty' && typeof payload.category === 'string') {
    return {
      category: payload.category,
      ...(typeof payload.reason === 'string' ? { reason: payload.reason } : {}),
    };
  }
  const empty = payload.empty;
  if (isRecord(empty) && typeof empty.category === 'string') {
    return {
      category: empty.category,
      ...(typeof empty.reason === 'string' ? { reason: empty.reason } : {}),
    };
  }
  return undefined;
}

/**
 * Top-level-declaration regex in the file's own idiom, for the
 * documentSymbols outline fallback. `^export` is a JS/TS-ism — pointing a
 * Rust or Python caller at it guarantees an empty follow-up search.
 */
function declarationRegexForFile(filePath: string): string {
  const ext = filePath.slice(filePath.lastIndexOf('.') + 1).toLowerCase();
  switch (ext) {
    case 'rs':
      return '^\\s*pub\\s';
    case 'py':
    case 'pyi':
      return '^\\s*(def|class)\\s';
    case 'go':
      return '^(func|type|var|const)\\s';
    case 'java':
    case 'kt':
    case 'scala':
    case 'cs':
      return '^\\s*(public|protected|internal)\\s';
    case 'rb':
      return '^\\s*(def|class|module)\\s';
    default:
      return '^\\s*export\\s';
  }
}

// The symbol to hand off to a text search. Symbol-anchored and workspaceSymbol
// queries carry `symbolName`; fall back to the resolved symbol's name.
function fallbackSymbolName(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope
): string | undefined {
  const fromQuery = (query as { symbolName?: unknown }).symbolName;
  if (typeof fromQuery === 'string' && fromQuery.trim())
    return fromQuery.trim();
  const fromResult = result.resolvedSymbol?.name;
  if (typeof fromResult === 'string' && fromResult.trim())
    return fromResult.trim();
  return undefined;
}

function localPathFromUri(uri: string): string {
  return uri.startsWith('file://')
    ? decodeURIComponent(uri.slice('file://'.length))
    : uri;
}

function looksLikeImportBinding(content: string, symbolName: string): boolean {
  if (/^\s*import(?:\s|\{|\*)/.test(content)) return true;
  const escapedName = symbolName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  // A server may return only the identifier line from a multiline import.
  // This recognizes a verification candidate; it never resolves or rewrites
  // the provider's target, and requires no guessed module path.
  return new RegExp(
    `^(?:type\\s+)?(?:${escapedName}|[$\\w]+\\s+as\\s+${escapedName})\\s*,?$`
  ).test(content.trim());
}

function paginationContinuation(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope
): NonNullable<LspSemanticEnvelope['next']>[string] | undefined {
  if (!isRecord(result.pagination)) return undefined;
  const { hasMore, nextPage } = result.pagination;
  if (
    hasMore !== true ||
    typeof nextPage !== 'number' ||
    nextPage > MAX_PAGE_NUMBER
  )
    return undefined;
  return buildNextPageContinuation(
    'lspGetSemantics',
    {
      ...query,
      page: nextPage,
      ...(typeof result.pagination.snapshot === 'string'
        ? { snapshot: result.pagination.snapshot }
        : {}),
    },
    `Continue semantic ${query.type} results on page ${nextPage}.`
  );
}

// Ready-to-run follow-up. On a hit: read the top result location with context,
// so the agent doesn't have to assemble the localGetFileContent call from
// ranges. On an empty/incomplete result: re-anchor or fall back to
// localSearch text, so the agent isn't left at a dead end.
export function withSemanticNext(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope | Record<string, unknown>
): LspSemanticEnvelope | Record<string, unknown> {
  if (!isSemanticEnvelope(result)) return result;
  let semanticResult: LspSemanticEnvelope = result;
  if (
    isRecord(semanticResult.pagination) &&
    semanticResult.pagination.hasMore === true &&
    typeof semanticResult.pagination.currentPage === 'number' &&
    semanticResult.pagination.currentPage >= MAX_PAGE_NUMBER
  ) {
    const { nextPage: _nextPage, ...pagination } = semanticResult.pagination;
    semanticResult = { ...semanticResult, pagination, terminalLimit: true };
  }
  const payload = semanticResult.payload as {
    locations?: Array<{
      uri?: string;
      content?: string;
      displayRange?: { startLine?: number; endLine?: number };
    }>;
    warmup?: { possiblyTruncated?: boolean; incompleteReasons?: string[] };
    completeness?: {
      truncatedByDepth?: boolean;
      truncatedByBudget?: boolean;
    };
  } & Record<string, unknown>;
  const warmupTruncated = payload.warmup?.possiblyTruncated === true;
  const depthTruncated = payload.completeness?.truncatedByDepth === true;
  const budgetTruncated = payload.completeness?.truncatedByBudget === true;
  const requestedDepth = 'depth' in query ? (query.depth ?? 1) : 1;
  const depthExpandable = depthTruncated && requestedDepth < 20;
  const symbolName = fallbackSymbolName(query, semanticResult);
  const completenessSearchPath =
    semanticResult.workspaceRoot ??
    query.workspaceRoot ??
    (semanticResult.uri ? localPathFromUri(semanticResult.uri) : undefined);
  const partialReasons = [
    ...(warmupTruncated
      ? ([
          payload.warmup?.incompleteReasons &&
          !payload.warmup.incompleteReasons.includes('fileCap')
            ? 'warmupIncomplete'
            : 'warmupCap',
        ] as const)
      : []),
    ...(depthTruncated ? (['depth'] as const) : []),
    ...(budgetTruncated ? (['budget'] as const) : []),
  ];
  if (partialReasons.length > 0) {
    semanticResult = {
      ...semanticResult,
      truncated: true,
      partialReasons,
      ...((budgetTruncated || (depthTruncated && !depthExpandable)) && {
        terminalLimit: true,
      }),
    };
  }
  const continuePage = paginationContinuation(query, semanticResult);
  const verificationRoot = semanticResult.workspaceRoot ?? query.workspaceRoot;
  const loc = payload.locations?.[0];
  const localBindingDefinition =
    query.type === 'definition' &&
    symbolName &&
    loc?.uri &&
    typeof loc.content === 'string' &&
    localPathFromUri(loc.uri) === localPathFromUri(semanticResult.uri) &&
    looksLikeImportBinding(loc.content, symbolName);
  const baseNext: NonNullable<LspSemanticEnvelope['next']> = {
    ...(semanticResult.next ?? {}),
    ...(continuePage ? { nextPage: continuePage } : {}),
    ...(localBindingDefinition
      ? {
          verifyDefinition: {
            tool: 'lspGetSemantics',
            query: {
              type: 'workspaceSymbol',
              uri: semanticResult.uri,
              symbolName,
              ...(verificationRoot && { workspaceRoot: verificationRoot }),
              ...(query.format && { format: query.format }),
            },
            why: 'Search workspace symbol candidates to verify this binding; a cold server may return the same import binding.',
            confidence: 'medium' as const,
          },
          ...(verificationRoot && {
            searchDefinitionCandidates: {
              tool: 'localSearch',
              query: {
                operation: 'text',
                path: verificationRoot,
                searchText: symbolName,
                regex: 'fixed',
                wholeWord: true,
                resultView: 'content',
                maxFiles: 100,
                pageSize: 20,
                maxMatchesPerFile: 20,
              },
              why: 'Find lexical declaration candidates when the language server only resolves the import binding.',
              confidence: 'low' as const,
            },
          }),
        }
      : {}),
    ...(depthExpandable
      ? {
          expandDepth: buildNextPageContinuation(
            'lspGetSemantics',
            {
              ...query,
              snapshot: undefined,
              depth: Math.min(
                20,
                Math.max(requestedDepth + 1, requestedDepth * 2)
              ),
              page: 1,
            },
            'Re-run with a larger call-hierarchy depth to inspect possible calls beyond the current depth.'
          ),
        }
      : {}),
    ...(warmupTruncated && symbolName && completenessSearchPath
      ? {
          verifyCompleteness: buildNextPageContinuation(
            'localSearch',
            {
              operation: 'text',
              path: completenessSearchPath,
              searchText: symbolName,
              wholeWord: true,
              resultView: 'files',
            },
            'The LSP consumer warmup was incomplete; search the complete workspace lexically for additional consumer files.'
          ),
        }
      : {}),
  };
  const start = loc?.displayRange?.startLine;
  if (loc?.uri && typeof start === 'number') {
    const path = localPathFromUri(loc.uri);
    return {
      ...semanticResult,
      next: {
        ...baseNext,
        readSite: {
          tool: 'localGetFileContent',
          query: {
            path,
            startLine: Math.max(1, start - 3),
            endLine: (loc.displayRange?.endLine ?? start) + 10,
          },
          why: 'Read the top result location with surrounding context',
          confidence: 'exact',
        },
      },
    };
  }

  // Empty/incomplete: only emit a fallback when there's a symbol to search for
  // and the empty state is one this fallback can actually help with.
  const empty = emptyStateOf(payload);

  // documentSymbols has no single symbolName to fall back on — it lists
  // every symbol in a file, not one — so the symbolName-gated fallback below
  // never fires for it, leaving a caller who hits an unsupported language
  // server (e.g. Flow-typed .js, no documentSymbolProvider) with no
  // pointer to the regex/AST outline workaround. Point at a FILE-scoped
  // declaration search instead of a symbol-scoped one, in the file's own
  // declaration idiom (`^export` finds nothing in Rust/Python/Go).
  if (
    empty?.category === 'unsupportedOperation' &&
    semanticResult.type === 'documentSymbols' &&
    semanticResult.uri
  ) {
    const filePath = localPathFromUri(semanticResult.uri);
    return {
      ...semanticResult,
      next: {
        ...baseNext,
        textSearch: {
          tool: 'localSearch',
          query: {
            operation: 'text',
            path: filePath,
            searchText: declarationRegexForFile(filePath),
            regex: 'perl',
          },
          why: "documentSymbols is unsupported for this file's language server — fall back to a regex search over top-level declarations for an outline.",
          confidence: 'low',
        },
      },
    };
  }

  if (!empty || !symbolName || !FALLBACK_EMPTY_CATEGORIES.has(empty.category)) {
    return Object.keys(baseNext).length > 0
      ? { ...semanticResult, next: baseNext }
      : semanticResult;
  }

  const searchUriOrRoot =
    semanticResult.type === 'workspaceSymbol'
      ? (semanticResult.workspaceRoot ??
        query.workspaceRoot ??
        semanticResult.uri ??
        query.uri)
      : (semanticResult.uri ??
        query.uri ??
        semanticResult.workspaceRoot ??
        query.workspaceRoot);
  if (!searchUriOrRoot) return semanticResult;
  const searchPath = localPathFromUri(searchUriOrRoot);
  const next: NonNullable<LspSemanticEnvelope['next']> = {
    ...baseNext,
    textSearch: {
      tool: 'localSearch',
      query: { operation: 'text', path: searchPath, searchText: symbolName },
      why: `Semantic ${semanticResult.type} returned no result (${empty.category}) — fall back to a text search for "${symbolName}"`,
      confidence: 'low',
    },
  };
  if (REANCHOR_EMPTY_CATEGORIES.has(empty.category) && semanticResult.uri) {
    next.reAnchor = {
      tool: 'lspGetSemantics',
      query: { type: 'documentSymbols', uri: semanticResult.uri },
      why: "Re-anchor: list this file's symbols to find the correct lineHint, then retry the semantic query",
      confidence: 'medium',
    };
  }
  return { ...semanticResult, next };
}
