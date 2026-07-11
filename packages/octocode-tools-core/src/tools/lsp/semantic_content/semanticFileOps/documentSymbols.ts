import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '@octocodeai/octocode-engine/lsp/manager';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import type { LSPRange } from '@octocodeai/octocode-engine/lsp/types';
import { markdownHeadingOutlineToDocumentSymbols } from '../../../../utils/markdownOutline.js';
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
  type SemanticEmptyCategory,
} from '../../shared/semanticTypes.js';
import { resolveFileAnchor } from '../../shared/resolveSymbolAnchor.js';
import {
  DEFAULT_SYMBOLS_PER_PAGE,
  paginateItems,
} from '../semanticEnvelopes.js';
import { symbolKindName } from '../semanticPresentation.js';
import { nativeDocumentSymbols, throwLspUnavailable } from './anchor.js';

export type CompactSymbol = {
  name: string;
  kind: string;
  line: number;
  character: number;
  endLine: number;
  childCount: number;
  containerName?: string;
};

type LspPositionLike = {
  line: number;
  character: number;
};

export async function getDocumentSymbols(
  query: LspGetSemanticsQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const anchor = await resolveFileAnchor(query, LSP_GET_SEMANTICS_TOOL_NAME);
  if (anchor.ok === false) return anchor.error;

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.uri));
  const serverAvailable = await isLanguageServerAvailable(
    anchor.value.uri,
    workspaceRoot
  );
  const client = serverAvailable
    ? await acquirePooledClient(workspaceRoot, anchor.value.uri)
    : null;
  const lspProvides = Boolean(client?.hasCapability('documentSymbolProvider'));

  // Source priority:
  //   1. Native OXC (JS/TS only) — always fast, no server round-trip.
  //      Preferred even when a server is available; avoids indexing-wait on
  //      documentSymbols for the most common file types.
  //   2. LSP server — for non-JS/TS languages with a documentSymbolProvider.
  //   3. Markdown heading outline — for .md files without a server.
  // Stamp `source` so callers know the fidelity tier.
  let symbols: unknown[] = [];
  let source: 'lsp' | 'native' | 'markdown' | undefined;
  const nativeFast = nativeDocumentSymbols(
    anchor.value.uri,
    anchor.value.content
  );
  if (nativeFast?.length) {
    symbols = nativeFast;
    source = 'native';
  } else if (lspProvides && client) {
    const raw = await client.documentSymbols(
      anchor.value.uri,
      anchor.value.content
    );
    symbols = Array.isArray(raw) ? raw : [];
    source = 'lsp';
  } else {
    const markdown = markdownHeadingOutlineToDocumentSymbols(
      anchor.value.content,
      anchor.value.uri
    );
    if (markdown) {
      symbols = markdown;
      source = 'markdown';
    }
  }

  const complete = source !== undefined;
  // No outline AND no server → throw (the agent should use text search). The
  // native (JS/TS) + markdown paths already ran above, so this only fires for
  // an unsupported language with no server.
  if (!complete && !serverAvailable) {
    throwLspUnavailable(anchor.value.uri, 'documentSymbols');
  }
  const compactSymbols = flattenDocumentSymbols(symbols);
  const topLevelSymbols = countTopLevelDocumentSymbols(symbols);
  const { pageItems, pagination } = paginateItems(
    compactSymbols,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );
  const kindCounts = countBy(compactSymbols, symbol => symbol.kind);
  // Server is present (checked above) but lacks documentSymbolProvider.
  const empty = complete
    ? undefined
    : {
        category: 'unsupportedOperation' as SemanticEmptyCategory,
        reason: 'documentSymbolProvider unsupported',
      };

  return {
    type: 'documentSymbols',
    uri: anchor.value.uri,
    lsp: {
      serverAvailable,
      ...(source === 'lsp' ? { provider: 'documentSymbolProvider' } : {}),
      ...(source ? { source } : {}),
    },
    summary: {
      totalSymbols: compactSymbols.length,
      returnedSymbols: pageItems.length,
      topLevelSymbols,
      kinds: kindCounts,
    },
    payload: {
      kind: 'documentSymbols',
      symbols: pageItems,
      ...(empty ? { empty } : {}),
    },
    pagination,
  };
}

export function flattenDocumentSymbols(
  symbols: readonly unknown[]
): CompactSymbol[] {
  const flattened: CompactSymbol[] = [];
  for (const symbol of symbols) {
    flattenDocumentSymbol(symbol, flattened);
  }
  return flattened.sort((a, b) => a.line - b.line || a.character - b.character);
}

export function flattenDocumentSymbol(
  value: unknown,
  output: CompactSymbol[],
  containerName?: string
): void {
  if (!value || typeof value !== 'object') return;
  const symbol = value as {
    name?: unknown;
    kind?: unknown;
    range?: unknown;
    location?: unknown;
    children?: unknown;
  };
  const range = getSymbolRange(symbol);
  if (typeof symbol.name === 'string' && range) {
    output.push({
      name: symbol.name,
      kind: symbolKindName(symbol.kind),
      line: range.start.line + 1,
      character: range.start.character,
      endLine: range.end.line + 1,
      childCount: Array.isArray(symbol.children) ? symbol.children.length : 0,
      ...(containerName ? { containerName } : {}),
    });
  }
  if (
    Array.isArray(symbol.children) &&
    STRUCTURAL_SYMBOL_KINDS.has(symbolKindName(symbol.kind))
  ) {
    const parentName =
      typeof symbol.name === 'string' ? symbol.name : containerName;
    for (const child of symbol.children) {
      flattenDocumentSymbol(child, output, parentName);
    }
  }
}

const STRUCTURAL_SYMBOL_KINDS = new Set([
  'file',
  'module',
  'namespace',
  'package',
  'class',
  'enum',
  'interface',
  'markdownHeading',
  'struct',
]);

export function getSymbolRange(value: {
  range?: unknown;
  location?: unknown;
}): LSPRange | undefined {
  if (isLspRange(value.range)) return value.range;
  const location = value.location as { range?: unknown } | undefined;
  return location && isLspRange(location.range) ? location.range : undefined;
}

export function isLspRange(value: unknown): value is LSPRange {
  if (!value || typeof value !== 'object') return false;
  const range = value as { start?: unknown; end?: unknown };
  return isPosition(range.start) && isPosition(range.end);
}

export function isPosition(value: unknown): value is LspPositionLike {
  if (!value || typeof value !== 'object') return false;
  const position = value as { line?: unknown; character?: unknown };
  return (
    typeof position.line === 'number' && typeof position.character === 'number'
  );
}

export function countTopLevelDocumentSymbols(
  symbols: readonly unknown[]
): number {
  return symbols.filter(
    symbol => symbol && typeof symbol === 'object' && 'name' in symbol
  ).length;
}

export function countBy<T>(
  items: readonly T[],
  keyForItem: (item: T) => string
): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const item of items) {
    const key = keyForItem(item);
    counts[key] = (counts[key] ?? 0) + 1;
  }
  return counts;
}
