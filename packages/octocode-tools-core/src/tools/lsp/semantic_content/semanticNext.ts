import {
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
} from '../shared/semanticTypes.js';
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

// Ready-to-run follow-up. On a hit: read the top result location with context,
// so the agent doesn't have to assemble the localGetFileContent call from
// ranges. On an empty/incomplete result: re-anchor or fall back to
// localSearchCode, so the agent isn't left at a dead end.
export function withSemanticNext(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope | Record<string, unknown>
): LspSemanticEnvelope | Record<string, unknown> {
  if (!isSemanticEnvelope(result)) return result;
  const payload = result.payload as {
    locations?: Array<{
      uri?: string;
      displayRange?: { startLine?: number; endLine?: number };
    }>;
  } & Record<string, unknown>;
  const loc = payload.locations?.[0];
  const start = loc?.displayRange?.startLine;
  if (loc?.uri && typeof start === 'number') {
    const path = loc.uri.startsWith('file://')
      ? decodeURIComponent(loc.uri.slice('file://'.length))
      : loc.uri;
    return {
      ...result,
      next: {
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
  const symbolName = fallbackSymbolName(query, result);
  if (!empty || !symbolName || !FALLBACK_EMPTY_CATEGORIES.has(empty.category)) {
    return result;
  }

  // NOTE: localSearchCode's `keywords` is a STRING (not an array).
  const next: NonNullable<LspSemanticEnvelope['next']> = {
    textSearch: {
      tool: 'localSearchCode',
      query: { keywords: symbolName },
      why: `Semantic ${result.type} returned no result (${empty.category}) — fall back to a text search for "${symbolName}"`,
      confidence: 'low',
    },
  };
  if (REANCHOR_EMPTY_CATEGORIES.has(empty.category) && result.uri) {
    next.reAnchor = {
      tool: 'lspGetSemantics',
      query: { type: 'documentSymbols', uri: result.uri },
      why: "Re-anchor: list this file's symbols to find the correct lineHint, then retry the semantic query",
      confidence: 'medium',
    };
  }
  return { ...result, next };
}
