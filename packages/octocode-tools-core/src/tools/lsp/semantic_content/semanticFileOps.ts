import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
  unavailableHintFor,
} from '@octocodeai/octocode-engine/lsp/manager';
import { detectLanguageId } from '@octocodeai/octocode-engine/lsp/config';
import { resolveWorkspaceRootForFile } from '@octocodeai/octocode-engine/lsp/workspaceRoot';
import type { LSPRange } from '@octocodeai/octocode-engine/lsp/types';
import { ToolError } from '../../../errors/ToolError.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../../errors/localToolErrors.js';
import { contextUtils } from '../../../utils/contextUtils.js';
import { markdownHeadingOutlineToDocumentSymbols } from '../../../utils/markdownOutline.js';
import {
  LSP_GET_SEMANTICS_TOOL_NAME,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
  type SemanticContentType,
  type SemanticEmptyCategory,
  type WorkspaceSymbolSemanticQuery,
  type DiagnosticSemanticQuery,
} from '../shared/semanticTypes.js';
import { resolveFileAnchor } from '../shared/resolveSymbolAnchor.js';
import {
  DEFAULT_SYMBOLS_PER_PAGE,
  paginateItems,
} from './semanticEnvelopes.js';
import { symbolKindName } from './semanticPresentation.js';

/**
 * Extensions oxc can outline natively (server-free, syntax-only). Sourced from
 * the engine (`getSupportedJsTsExtensions`) so the dispatch list never drifts
 * from the Rust guard; dotted + cached for `path.extname` comparison.
 */
let nativeJsTsExtsCache: Set<string> | undefined;
export function isNativeJsTsFile(uri: string): boolean {
  if (!nativeJsTsExtsCache) {
    nativeJsTsExtsCache = new Set(
      contextUtils.getSupportedJsTsExtensions().map(ext => `.${ext}`)
    );
  }
  return nativeJsTsExtsCache.has(path.extname(uri).toLowerCase());
}

/**
 * Throw when a real language server cannot answer a semantic operation. We do
 * NOT fabricate a syntactic/same-file stand-in: a faked answer is worse than an
 * honest failure because the agent would trust it. The thrown ToolError is
 * routed by the execution boundary into the standard `status:"error"` envelope
 * (errorCode `lspServerUnavailable`), and the message directs the agent to text
 * search instead. documentSymbols/structural search keep their tree-sitter path
 * and never reach here.
 */
export function throwLspUnavailable(
  uri: string,
  op: SemanticContentType
): never {
  const languageId = detectLanguageId(uri);
  const hint = unavailableHintFor(languageId, undefined);
  throw new ToolError(
    LOCAL_TOOL_ERROR_CODES.LSP_SERVER_UNAVAILABLE,
    `No ${languageId} language server is available for ${uri}, so "${op}" cannot be answered semantically. ${hint} ` +
      `Meanwhile, use localSearchCode (text or structural search) to find the symbol's occurrences and localGetFileContent to read the surrounding code.`
  );
}

const WORKSPACE_SYMBOL_FALLBACK_EXTENSIONS = [
  'py',
  'rs',
  'go',
  'java',
  'kt',
  'cs',
  'c',
  'cc',
  'cpp',
  'h',
  'hpp',
  'rb',
  'php',
  'swift',
  'scala',
  'lua',
  'dart',
  'ex',
  'exs',
  'erl',
  'hrl',
  'clj',
  'cljs',
] as const;

export function toLocalPath(value: string, workspaceRoot: string): string {
  const filePath = value.startsWith('file://') ? fileURLToPath(value) : value;
  return path.isAbsolute(filePath)
    ? filePath
    : path.resolve(workspaceRoot, filePath);
}

export function workspaceSymbolAnchorExtensions(): string[] {
  return [
    ...contextUtils.getSupportedJsTsExtensions(),
    ...WORKSPACE_SYMBOL_FALLBACK_EXTENSIONS,
  ];
}

export function workspaceSymbolAnchorIncludeGlobs(): string[] {
  return workspaceSymbolAnchorExtensions().map(ext => `**/*.${ext}`);
}

const WORKSPACE_SYMBOL_EXCLUDE_DIRS = [
  '.git',
  'node_modules',
  'dist',
  'out',
  'coverage',
  'target',
] as const;

export async function findWorkspaceSymbolAnchorByName(
  query: WorkspaceSymbolSemanticQuery,
  workspaceRoot: string
): Promise<string | undefined> {
  const symbolName = query.symbolName?.trim();
  if (!symbolName) return undefined;
  try {
    const result = await contextUtils.searchRipgrep({
      path: workspaceRoot,
      pattern: symbolName,
      fixedString: true,
      caseSensitive: true,
      filesOnly: true,
      include: workspaceSymbolAnchorIncludeGlobs(),
      excludeDir: [...WORKSPACE_SYMBOL_EXCLUDE_DIRS],
      maxSnippetChars: 1,
    });
    return result.files[0]?.path;
  } catch {
    return undefined;
  }
}

export async function resolveWorkspaceSymbolAnchor(
  query: WorkspaceSymbolSemanticQuery,
  workspaceRoot: string
): Promise<string> {
  if (query.uri) return toLocalPath(query.uri, workspaceRoot);
  const symbolHit = await findWorkspaceSymbolAnchorByName(query, workspaceRoot);
  if (symbolHit) return symbolHit;
  try {
    const result = contextUtils.queryFileSystem({
      path: workspaceRoot,
      recursive: true,
      includeRoot: false,
      showHidden: false,
      entryType: 'f',
      extensions: workspaceSymbolAnchorExtensions(),
      maxDepth: 5,
      limit: 1,
    });
    const first = result.entries[0];
    if (first) return first.path;
  } catch {
    // Fall back to the root; the language-server availability check returns a
    // structured serverUnavailable envelope if no source-file anchor exists.
  }
  return workspaceRoot;
}

export function lspErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Native JS/TS document symbols via oxc, parsed into the LSP `DocumentSymbol[]`
 * shape. Returns `null` when oxc declines the input so the caller can fall back
 * to the "no symbols" empty state.
 */
export function nativeDocumentSymbols(
  uri: string,
  content: string
): unknown[] | null {
  if (!isNativeJsTsFile(uri)) return null;
  try {
    const json = contextUtils.extractJsSymbols(content, uri);
    if (!json) return null;
    const parsed = JSON.parse(json);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

export type CompactSymbol = {
  name: string;
  kind: string;
  line: number;
  character: number;
  endLine: number;
  childCount: number;
  containerName?: string;
};

type CompactWorkspaceSymbol = CompactSymbol & { uri: string };

type LspPositionLike = {
  line: number;
  character: number;
};

type DiagnosticItem = {
  severity?: number;
  message: string;
  line: number;
  endLine: number;
  character: number;
  code?: string | number;
  source?: string;
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

export async function getWorkspaceSymbols(
  query: WorkspaceSymbolSemanticQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const symbolQuery = query.symbolName ?? '';
  const workspaceRoot = path.resolve(query.workspaceRoot ?? process.cwd());

  // workspace/symbol is project-wide, but language-server selection is
  // extension-based. Use an explicit uri when provided; otherwise pick a
  // representative source file under the workspace root.
  const anchorFile = await resolveWorkspaceSymbolAnchor(query, workspaceRoot);
  const serverAvailable = await isLanguageServerAvailable(
    anchorFile,
    workspaceRoot
  );
  if (!serverAvailable) {
    throwLspUnavailable(anchorFile, 'workspaceSymbol');
  }

  const client = await acquirePooledClient(workspaceRoot, anchorFile);
  if (!client) {
    throwLspUnavailable(anchorFile, 'workspaceSymbol');
  }

  if (!client.hasCapability('workspaceSymbolProvider')) {
    return {
      type: 'workspaceSymbol',
      uri: anchorFile,
      lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason: 'workspaceSymbolProvider unsupported',
      },
    } satisfies LspSemanticEnvelope;
  }

  let raw: unknown[];
  try {
    if (path.extname(anchorFile)) {
      await client.openDocument(anchorFile);
    }
    raw = await client.workspaceSymbol(symbolQuery);
  } catch (error) {
    return {
      type: 'workspaceSymbol',
      uri: anchorFile,
      lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason: `workspaceSymbolProvider failed: ${lspErrorMessage(error)}`,
      },
    } satisfies LspSemanticEnvelope;
  }
  const symbols = compactWorkspaceSymbols(raw);
  const { pageItems, pagination } = paginateItems(
    symbols,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );

  return {
    type: 'workspaceSymbol',
    uri: anchorFile,
    lsp: { serverAvailable: true, provider: 'workspaceSymbolProvider' },
    summary: { query: symbolQuery, totalSymbols: symbols.length },
    payload:
      symbols.length > 0
        ? {
            kind: 'workspaceSymbol',
            query: symbolQuery,
            symbols: pageItems,
            totalSymbols: symbols.length,
          }
        : {
            kind: 'empty',
            category: 'noWorkspaceSymbols',
            reason: `workspaceSymbolProvider returned no symbols for query "${symbolQuery}"`,
          },
    pagination,
  } satisfies LspSemanticEnvelope;
}

export function compactWorkspaceSymbols(
  raw: unknown[]
): CompactWorkspaceSymbol[] {
  return raw.flatMap(item => {
    if (!item || typeof item !== 'object') return [];
    const sym = item as Record<string, unknown>;
    const name = typeof sym['name'] === 'string' ? sym['name'] : undefined;
    if (!name) return [];
    const kind = sym['kind'];
    // WorkspaceSymbol has `location.uri + location.range`; SymbolInformation same shape.
    const loc = sym['location'] as Record<string, unknown> | undefined;
    const range = loc?.['range'] as
      | {
          start?: { line?: number; character?: number };
          end?: { line?: number };
        }
      | undefined;
    const uri = typeof loc?.['uri'] === 'string' ? loc['uri'] : '';
    const line = (range?.start?.line ?? 0) + 1;
    const endLine = (range?.end?.line ?? range?.start?.line ?? 0) + 1;
    const containerName =
      typeof sym['containerName'] === 'string'
        ? sym['containerName']
        : undefined;
    return [
      {
        name,
        kind: symbolKindName(kind),
        line,
        character: range?.start?.character ?? 0,
        endLine,
        childCount: 0,
        ...(containerName ? { containerName } : {}),
        uri,
      },
    ];
  });
}

export async function getFileDiagnostics(
  query: DiagnosticSemanticQuery
): Promise<LspSemanticEnvelope | Record<string, unknown>> {
  const uri = query.uri ?? '';
  const workspaceRoot =
    query.workspaceRoot ??
    (uri ? await resolveWorkspaceRootForFile(uri) : process.cwd());

  const serverAvailable = await isLanguageServerAvailable(uri, workspaceRoot);
  if (!serverAvailable) {
    throwLspUnavailable(uri, 'diagnostic');
  }

  const client = await acquirePooledClient(workspaceRoot, uri);
  if (!client) {
    throwLspUnavailable(uri, 'diagnostic');
  }

  if (!client.hasCapability('diagnosticProvider')) {
    return {
      type: 'diagnostic',
      uri,
      lsp: { serverAvailable: true, provider: 'diagnosticProvider' },
      payload: {
        kind: 'empty',
        category: 'unsupportedOperation',
        reason:
          'diagnosticProvider (pull) unsupported — server uses push (publishDiagnostics) instead',
      },
      warnings: [
        'This server pushes diagnostics via textDocument/publishDiagnostics. ' +
          'Pull diagnostics (type: "diagnostic") require LSP 3.17 pull support. ' +
          'Check server docs to enable it.',
      ],
    } satisfies LspSemanticEnvelope;
  }

  const raw = await client.getDiagnostics(uri);
  const diags = extractDiagnostics(raw);
  const errorCount = diags.filter(d => d.severity === 1).length;
  const warningCount = diags.filter(d => d.severity === 2).length;

  const { pageItems, pagination } = paginateItems(
    diags,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_SYMBOLS_PER_PAGE
  );

  return {
    type: 'diagnostic',
    uri,
    lsp: { serverAvailable: true, provider: 'diagnosticProvider' },
    summary: {
      totalDiagnostics: diags.length,
      errorCount,
      warningCount,
    },
    payload:
      diags.length > 0
        ? {
            kind: 'diagnostic',
            diagnostics: pageItems,
            totalDiagnostics: diags.length,
            errorCount,
            warningCount,
          }
        : {
            kind: 'empty',
            category: 'noDiagnostics',
            reason: 'No diagnostics — file has no errors or warnings',
          },
    pagination,
  } satisfies LspSemanticEnvelope;
}

export function extractDiagnostics(raw: unknown): DiagnosticItem[] {
  // Pull response shape: { kind: "full", items: Diagnostic[] }
  if (raw && typeof raw === 'object') {
    const report = raw as Record<string, unknown>;
    const items = Array.isArray(report['items']) ? report['items'] : [];
    return items.flatMap(item => parseDiagnostic(item));
  }
  return [];
}

export function parseDiagnostic(item: unknown): DiagnosticItem[] {
  if (!item || typeof item !== 'object') return [];
  const d = item as Record<string, unknown>;
  const range = d['range'] as
    | { start?: { line?: number; character?: number }; end?: { line?: number } }
    | undefined;
  const message = typeof d['message'] === 'string' ? d['message'] : '';
  if (!message) return [];
  return [
    {
      severity: typeof d['severity'] === 'number' ? d['severity'] : undefined,
      message,
      line: (range?.start?.line ?? 0) + 1,
      endLine: (range?.end?.line ?? range?.start?.line ?? 0) + 1,
      character: range?.start?.character ?? 0,
      ...(d['code'] !== undefined
        ? { code: d['code'] as string | number }
        : {}),
      ...(typeof d['source'] === 'string' ? { source: d['source'] } : {}),
    },
  ];
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
