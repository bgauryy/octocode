import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { executeBulkOperation } from '../../../utils/response/bulk.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../../utils/response/charSavings.js';
import type { ToolExecutionArgs } from '../../../types/execution.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
} from '../../../lsp/manager.js';
import { resolveWorkspaceRootForFile } from '../../../lsp/workspaceRoot.js';
import type {
  CallHierarchyItem,
  CodeSnippet,
  IncomingCall,
  LSPRange,
  OutgoingCall,
  ReferenceLocation,
  ReferencesByFile,
} from '../../../lsp/types.js';
import {
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
  createCallItemKey,
} from '../shared/callHierarchyTraversal.js';
import {
  LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
  type LspGetSemanticContentQuery,
  type LspSemanticEnvelope,
  type SemanticContentType,
  type SymbolAnchoredSemanticQuery,
} from '../shared/semanticTypes.js';
import {
  resolveFileAnchor,
  resolveSymbolAnchor,
  type SymbolAnchor,
} from '../shared/resolveSymbolAnchor.js';
import { semanticHints } from './hints.js';

const DEFAULT_SYMBOLS_PER_PAGE = 40;
const DEFAULT_CALLS_PER_PAGE = 10;
const MAX_CONTENT_PREVIEW_CHARS = 1_200;
const MAX_RANGE_SAMPLES = 8;

type PaginationInfo = {
  currentPage: number;
  totalPages: number;
  totalResults: number;
  hasMore: boolean;
  itemsPerPage: number;
  nextPage?: number;
};

type CompactSymbol = {
  name: string;
  kind: string;
  line: number;
  character: number;
  endLine: number;
  childCount: number;
  containerName?: string;
};

type CompactCallTarget = {
  name: string;
  kind: string;
  uri: string;
  line: number;
  endLine: number;
  selectionLine?: number;
};

type CompactCall = {
  direction: 'incoming' | 'outgoing';
  item: CompactCallTarget;
  ranges: Array<{ line: number; character: number }>;
  rangeCount: number;
  rangeSampleCount: number;
  contentPreview?: string;
};

type LspPositionLike = {
  line: number;
  character: number;
};

export async function executeLspGetSemanticContent(
  args: ToolExecutionArgs<LspGetSemanticContentQuery>
): Promise<CallToolResult> {
  return executeBulkOperation(
    args.queries || [],
    async query =>
      executeWithToolBoundary({
        toolName: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
        query,
        contextMessage: 'lspGetSemanticContent execution failed',
        execute: async () =>
          attachSemanticRawEvidence(await getSemanticContent(query)),
      }),
    {
      toolName: LSP_GET_SEMANTIC_CONTENT_TOOL_NAME,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    },
    args
  );
}

function attachSemanticRawEvidence<T extends object>(result: T): T {
  return attachRawResponseChars(result, countSerializedChars(result));
}

async function getSemanticContent(
  query: LspGetSemanticContentQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  if (query.type === 'documentSymbols') {
    return getDocumentSymbols(query);
  }

  const anchor = await resolveSymbolAnchor(
    query,
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
  );
  if (anchor.ok === false) {
    const message =
      typeof anchor.error.error === 'string'
        ? anchor.error.error
        : 'Symbol anchor resolution failed';
    const anchorHints = Array.isArray(anchor.error.hints)
      ? (anchor.error.hints as string[])
      : undefined;
    return failedAnchorEnvelope(query, message, anchorHints);
  }

  const workspaceRoot =
    query.workspaceRoot ??
    (await resolveWorkspaceRootForFile(anchor.value.uri));
  const serverAvailable = await isLanguageServerAvailable(
    anchor.value.uri,
    workspaceRoot
  );
  if (!serverAvailable) {
    return emptyEnvelope(
      query.type,
      anchor.value,
      'Language server unavailable'
    );
  }

  const client = await acquirePooledClient(workspaceRoot, anchor.value.uri);
  if (!client) {
    return emptyEnvelope(
      query.type,
      anchor.value,
      'Language server unavailable'
    );
  }

  switch (query.type) {
    case 'definition':
      if (!client.hasCapability('definitionProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'definitionProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor.value,
        'definition',
        'definitionProvider',
        await client.gotoDefinition(
          anchor.value.uri,
          anchor.value.resolvedSymbol.position,
          anchor.value.content
        )
      );
    case 'typeDefinition':
      if (!client.hasCapability('typeDefinitionProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'typeDefinitionProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor.value,
        'typeDefinition',
        'typeDefinitionProvider',
        await client.typeDefinition(
          anchor.value.uri,
          anchor.value.resolvedSymbol.position,
          anchor.value.content
        )
      );
    case 'implementation':
      if (!client.hasCapability('implementationProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'implementationProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor.value,
        'implementation',
        'implementationProvider',
        await client.implementation(
          anchor.value.uri,
          anchor.value.resolvedSymbol.position,
          anchor.value.content
        )
      );
    case 'references':
      if (!client.hasCapability('referencesProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'referencesProvider unsupported',
          true
        );
      }
      return referencesEnvelope(
        query,
        anchor.value,
        await client.findReferences(
          anchor.value.uri,
          anchor.value.resolvedSymbol.position,
          query.includeDeclaration ?? true,
          anchor.value.content
        )
      );
    case 'hover':
      if (!client.hasCapability('hoverProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'hoverProvider unsupported',
          true
        );
      }
      return hoverEnvelope(
        query,
        anchor.value,
        await client.hover(
          anchor.value.uri,
          anchor.value.resolvedSymbol.position,
          anchor.value.content
        )
      );
    case 'callers':
    case 'callees':
    case 'callHierarchy':
      if (!client.hasCapability('callHierarchyProvider')) {
        return emptyEnvelope(
          query.type,
          anchor.value,
          'callHierarchyProvider unsupported',
          true
        );
      }
      return callsEnvelope(query, anchor.value, client);
  }
}

async function getDocumentSymbols(
  query: LspGetSemanticContentQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const anchor = await resolveFileAnchor(
    query,
    LSP_GET_SEMANTIC_CONTENT_TOOL_NAME
  );
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
  const symbols = client
    ? client.hasCapability('documentSymbolProvider')
      ? await client.documentSymbols(anchor.value.uri, anchor.value.content)
      : []
    : [];
  const complete = Boolean(client?.hasCapability('documentSymbolProvider'));
  const compactSymbols = flattenDocumentSymbols(symbols);
  const { pageItems, pagination } = paginateItems(
    compactSymbols,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );
  const kindCounts = countBy(pageItems, symbol => symbol.kind);

  return {
    type: 'documentSymbols',
    uri: anchor.value.uri,
    lsp: {
      serverAvailable,
      ...(complete ? { provider: 'documentSymbolProvider' } : {}),
    },
    evidence: {
      confidence: complete ? 'high' : 'low',
      complete,
      reason: complete
        ? undefined
        : serverAvailable
          ? 'documentSymbolProvider unsupported'
          : 'Language server unavailable',
    },
    summary: {
      totalSymbols: compactSymbols.length,
      returnedSymbols: pageItems.length,
      topLevelSymbols: countTopLevelDocumentSymbols(symbols),
      kinds: kindCounts,
    },
    payload: {
      kind: 'documentSymbols',
      symbols: pageItems,
      totalSymbols: compactSymbols.length,
      topLevelSymbols: countTopLevelDocumentSymbols(symbols),
    },
    pagination,
    warnings: complete
      ? undefined
      : [
          serverAvailable
            ? 'documentSymbolProvider unsupported'
            : 'Language server unavailable',
        ],
    hints: semanticHints('documentSymbols', complete),
  };
}

function locationsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  kind: 'definition' | 'typeDefinition' | 'implementation',
  provider: string,
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const complete = locations.length > 0;
  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: anchor.resolvedSymbol,
    lsp: { serverAvailable: true, provider },
    evidence: {
      confidence: complete ? 'high' : 'medium',
      complete,
      reason: complete ? undefined : `${provider} returned no locations`,
    },
    payload: complete
      ? { kind, locations }
      : { kind: 'empty', reason: `${provider} returned no locations` },
    hints: semanticHints(query.type, complete),
  };
}

function referencesEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const refs = locations.map((location): ReferenceLocation => {
    const isDefinition =
      location.uri === anchor.uri &&
      location.range.start.line === anchor.resolvedSymbol.position.line &&
      location.range.start.character ===
        anchor.resolvedSymbol.position.character;
    return { ...location, ...(isDefinition ? { isDefinition: true } : {}) };
  });
  const byFile = query.groupByFile ? buildReferencesByFile(refs) : undefined;

  return {
    type: 'references',
    uri: anchor.uri,
    resolvedSymbol: anchor.resolvedSymbol,
    lsp: { serverAvailable: true, provider: 'referencesProvider' },
    evidence: {
      confidence: refs.length > 0 ? 'high' : 'medium',
      complete: true,
      reason:
        refs.length > 0
          ? undefined
          : 'referencesProvider returned no references',
    },
    payload: {
      kind: 'references',
      locations: refs,
      byFile,
      totalReferences: refs.length,
      totalFiles: new Set(refs.map(ref => ref.uri)).size,
    },
    hints: semanticHints('references', true),
  };
}

async function hoverEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  hover: unknown
): Promise<LspSemanticEnvelope> {
  const normalized = normalizeHover(hover);
  const complete = Boolean(normalized.markdown || normalized.text);

  return {
    type: 'hover',
    uri: anchor.uri,
    resolvedSymbol: anchor.resolvedSymbol,
    lsp: { serverAvailable: true, provider: 'hoverProvider' },
    evidence: {
      confidence: complete ? 'high' : 'medium',
      complete,
      reason: complete ? undefined : 'hoverProvider returned no hover content',
    },
    payload: complete
      ? { kind: 'hover', ...normalized }
      : { kind: 'empty', reason: 'hoverProvider returned no hover content' },
    hints: semanticHints(query.type, complete),
  };
}

async function callsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>
): Promise<LspSemanticEnvelope> {
  const items = await client.prepareCallHierarchy(
    anchor.uri,
    anchor.resolvedSymbol.position,
    anchor.content
  );
  const root = items[0];
  if (!root) {
    return emptyEnvelope(query.type, anchor, 'No callable symbol found', true);
  }

  const depth = query.depth ?? 1;
  const emptyTraversal = {
    calls: [],
    truncatedByDepth: false,
    cycleCount: 0,
    failedRequestCount: 0,
  } as const;
  const incomingResult =
    query.type === 'callers' || query.type === 'callHierarchy'
      ? await gatherIncomingCallsRecursive(
          client,
          root,
          depth,
          new Set([createCallItemKey(root)]),
          query.contextLines ?? 0
        )
      : emptyTraversal;
  const outgoingResult =
    query.type === 'callees' || query.type === 'callHierarchy'
      ? await gatherOutgoingCallsRecursive(
          client,
          root,
          depth,
          new Set([createCallItemKey(root)]),
          query.contextLines ?? 0
        )
      : emptyTraversal;

  const calls = [
    ...incomingResult.calls.map(call => ({
      direction: 'incoming' as const,
      ...call,
    })),
    ...outgoingResult.calls.map(call => ({
      direction: 'outgoing' as const,
      ...call,
    })),
  ];
  const compactCalls = calls.map(call =>
    call.direction === 'incoming'
      ? compactIncomingCall(call, query.contextLines ?? 0)
      : compactOutgoingCall(call, query.contextLines ?? 0)
  );
  const { pageItems, pagination } = paginateItems(
    compactCalls,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_CALLS_PER_PAGE
  );
  const direction =
    query.type === 'callers'
      ? 'incoming'
      : query.type === 'callees'
        ? 'outgoing'
        : 'both';

  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: anchor.resolvedSymbol,
    lsp: { serverAvailable: true, provider: 'callHierarchyProvider' },
    evidence: {
      confidence: calls.length > 0 ? 'high' : 'medium',
      complete: true,
      reason:
        calls.length > 0
          ? undefined
          : 'callHierarchyProvider returned no calls',
    },
    payload: {
      kind: query.type as 'callers' | 'callees' | 'callHierarchy',
      root: compactCallItem(root),
      direction,
      calls: pageItems,
      totalCalls: compactCalls.length,
      completeness: {
        complete: compactCalls.length > 0,
        truncatedByDepth:
          incomingResult.truncatedByDepth || outgoingResult.truncatedByDepth,
        cycleCount: incomingResult.cycleCount + outgoingResult.cycleCount,
        failedRequestCount:
          incomingResult.failedRequestCount + outgoingResult.failedRequestCount,
        dynamicCallsExcluded: true,
      },
    },
    summary: {
      totalCalls: compactCalls.length,
      returnedCalls: pageItems.length,
      direction,
      incomingCalls: incomingResult.calls.length,
      outgoingCalls: outgoingResult.calls.length,
    },
    pagination,
    hints: [
      ...semanticHints(query.type, true),
      ...(pagination.hasMore
        ? [`More calls available — retry with page=${pagination.nextPage}.`]
        : []),
      ...(query.contextLines && query.contextLines > 0
        ? []
        : [
            'Set contextLines>0 to include source previews for returned calls.',
          ]),
    ],
  };
}

function paginateItems<T>(
  items: readonly T[],
  requestedPage: number,
  requestedItemsPerPage: number
): { pageItems: T[]; pagination: PaginationInfo } {
  const itemsPerPage = Math.max(1, requestedItemsPerPage);
  const totalPages = Math.max(1, Math.ceil(items.length / itemsPerPage));
  const currentPage = Math.min(Math.max(1, requestedPage), totalPages);
  const start = (currentPage - 1) * itemsPerPage;
  const pageItems = items.slice(start, start + itemsPerPage);
  const hasMore = currentPage < totalPages;

  return {
    pageItems,
    pagination: {
      currentPage,
      totalPages,
      totalResults: items.length,
      hasMore,
      itemsPerPage,
      ...(hasMore ? { nextPage: currentPage + 1 } : {}),
    },
  };
}

function flattenDocumentSymbols(symbols: readonly unknown[]): CompactSymbol[] {
  const flattened: CompactSymbol[] = [];
  for (const symbol of symbols) {
    flattenDocumentSymbol(symbol, flattened);
  }
  return flattened.sort((a, b) => a.line - b.line || a.character - b.character);
}

function flattenDocumentSymbol(
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
  if (Array.isArray(symbol.children)) {
    const parentName =
      typeof symbol.name === 'string' ? symbol.name : containerName;
    for (const child of symbol.children) {
      flattenDocumentSymbol(child, output, parentName);
    }
  }
}

function getSymbolRange(value: {
  range?: unknown;
  location?: unknown;
}): LSPRange | undefined {
  if (isLspRange(value.range)) return value.range;
  const location = value.location as { range?: unknown } | undefined;
  return location && isLspRange(location.range) ? location.range : undefined;
}

function isLspRange(value: unknown): value is LSPRange {
  if (!value || typeof value !== 'object') return false;
  const range = value as { start?: unknown; end?: unknown };
  return isPosition(range.start) && isPosition(range.end);
}

function isPosition(value: unknown): value is LspPositionLike {
  if (!value || typeof value !== 'object') return false;
  const position = value as { line?: unknown; character?: unknown };
  return (
    typeof position.line === 'number' && typeof position.character === 'number'
  );
}

function countTopLevelDocumentSymbols(symbols: readonly unknown[]): number {
  return symbols.filter(
    symbol => symbol && typeof symbol === 'object' && 'name' in symbol
  ).length;
}

function countBy<T>(
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

function compactIncomingCall(
  call: IncomingCall & { direction: 'incoming' },
  contextLines: number
): CompactCall {
  const ranges = compactRanges(call.fromRanges);
  return {
    direction: 'incoming',
    item: compactCallItem(call.from),
    ranges,
    rangeCount: call.fromRanges.length,
    rangeSampleCount: ranges.length,
    ...contentPreview(call.from, contextLines),
  };
}

function compactOutgoingCall(
  call: OutgoingCall & { direction: 'outgoing' },
  contextLines: number
): CompactCall {
  const ranges = compactRanges(call.fromRanges);
  return {
    direction: 'outgoing',
    item: compactCallItem(call.to),
    ranges,
    rangeCount: call.fromRanges.length,
    rangeSampleCount: ranges.length,
    ...contentPreview(call.to, contextLines),
  };
}

function compactCallItem(item: CallHierarchyItem): CompactCallTarget {
  return {
    name: item.name,
    kind: item.kind,
    uri: item.uri,
    line: item.range.start.line + 1,
    endLine: item.range.end.line + 1,
    ...(item.selectionRange
      ? { selectionLine: item.selectionRange.start.line + 1 }
      : {}),
  };
}

function compactRanges(ranges: readonly LSPRange[]) {
  const seen = new Set<string>();
  const compact: Array<{ line: number; character: number }> = [];
  for (const range of ranges) {
    const line = range.start.line + 1;
    const character = range.start.character;
    const key = `${line}:${character}`;
    if (seen.has(key)) continue;
    seen.add(key);
    compact.push({ line, character });
    if (compact.length >= MAX_RANGE_SAMPLES) break;
  }
  return compact;
}

function contentPreview(
  item: CallHierarchyItem,
  contextLines: number
): { contentPreview?: string } {
  if (contextLines <= 0 || !item.content) return {};
  return { contentPreview: truncateContent(item.content) };
}

function truncateContent(content: string): string {
  if (content.length <= MAX_CONTENT_PREVIEW_CHARS) return content;
  return `${content.slice(0, MAX_CONTENT_PREVIEW_CHARS)}\n[truncated]`;
}

function symbolKindName(kind: unknown): string {
  const numericKind = typeof kind === 'number' ? kind : undefined;
  switch (numericKind) {
    case 1:
      return 'file';
    case 2:
      return 'module';
    case 3:
      return 'namespace';
    case 4:
      return 'package';
    case 5:
      return 'class';
    case 6:
      return 'method';
    case 7:
      return 'property';
    case 8:
      return 'field';
    case 9:
      return 'constructor';
    case 10:
      return 'enum';
    case 11:
      return 'interface';
    case 12:
      return 'function';
    case 13:
      return 'variable';
    case 14:
      return 'constant';
    case 15:
      return 'string';
    case 16:
      return 'number';
    case 17:
      return 'boolean';
    case 18:
      return 'array';
    case 19:
      return 'object';
    case 20:
      return 'key';
    case 21:
      return 'null';
    case 22:
      return 'enumMember';
    case 23:
      return 'struct';
    case 24:
      return 'event';
    case 25:
      return 'operator';
    case 26:
      return 'typeParameter';
    default:
      return 'unknown';
  }
}

function failedAnchorEnvelope(
  query: LspGetSemanticContentQuery,
  reason: string,
  hints?: string[]
): LspSemanticEnvelope {
  const uri = query.uri ?? query.filePath ?? '';
  return {
    type: query.type,
    uri,
    // serverAvailable is omitted: symbol resolution failed before reaching the LSP server,
    // so server availability is unknown. Presence of reason/warnings conveys the real issue.
    lsp: {},
    evidence: { confidence: 'low', complete: false, reason },
    payload: { kind: 'empty', reason },
    warnings: [reason],
    hints: [...semanticHints(query.type, false), ...(hints ?? [])],
  };
}

function emptyEnvelope(
  type: SemanticContentType,
  anchor: SymbolAnchor,
  reason: string,
  serverAvailable = false
): LspSemanticEnvelope {
  return {
    type,
    uri: anchor.uri,
    resolvedSymbol: anchor.resolvedSymbol,
    lsp: { serverAvailable },
    evidence: { confidence: 'low', complete: false, reason },
    payload: { kind: 'empty', reason },
    warnings: [reason],
    hints: semanticHints(type, false),
  };
}

function buildReferencesByFile(
  locations: readonly ReferenceLocation[]
): ReferencesByFile[] {
  const byUri = new Map<string, ReferencesByFile>();
  for (const loc of locations) {
    const lineNumber = loc.range.start.line + 1;
    const existing = byUri.get(loc.uri);
    if (existing) {
      existing.count += 1;
      existing.lines.push(lineNumber);
      if (loc.isDefinition) existing.hasDefinition = true;
      continue;
    }
    byUri.set(loc.uri, {
      uri: loc.uri,
      count: 1,
      firstLine: lineNumber,
      firstCharacter: loc.range.start.character,
      lines: [lineNumber],
      ...(loc.isDefinition ? { hasDefinition: true } : {}),
    });
  }
  return [...byUri.values()];
}

function normalizeHover(hover: unknown): {
  markdown?: string;
  text?: string;
  range?: LSPRange;
} {
  if (!hover || typeof hover !== 'object') return {};
  const value = hover as { contents?: unknown; range?: unknown };
  const content = value.contents;
  if (typeof content === 'string') return { text: content };
  if (Array.isArray(content)) {
    return {
      markdown: content.map(part => stringifyHoverPart(part)).join('\n'),
    };
  }
  if (content && typeof content === 'object') {
    const part = content as { kind?: unknown; value?: unknown };
    if (typeof part.value === 'string') {
      return part.kind === 'markdown'
        ? { markdown: part.value }
        : { text: part.value };
    }
  }
  return {};
}

function stringifyHoverPart(part: unknown): string {
  if (typeof part === 'string') return part;
  if (part && typeof part === 'object') {
    const value = (part as { value?: unknown }).value;
    if (typeof value === 'string') return value;
  }
  return String(part);
}
