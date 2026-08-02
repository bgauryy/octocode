import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unavailableHintFor } from '@octocodeai/octocode-engine/lsp/manager';
import { detectLanguageId } from '@octocodeai/octocode-engine/lsp/config';
import { ToolError } from '../../../../errors/ToolError.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../../../errors/localToolErrors.js';
import { contextUtils } from '../../../../utils/contextUtils.js';
import { isValidJsSymbolName } from '../../../../utils/jsSymbolNames.js';
import type {
  SemanticContentType,
  WorkspaceSymbolSemanticQuery,
} from '../../shared/semanticTypes.js';

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
  op: SemanticContentType,
  detail?: { kind?: string; message?: string }
): never {
  const languageId = detectLanguageId(uri);
  const hint = unavailableHintFor(languageId, undefined);
  const startupDetail = detail?.message
    ? ` LSP startup detail (${detail.kind ?? 'startupFailed'}): ${detail.message}.`
    : '';
  throw new ToolError(
    LOCAL_TOOL_ERROR_CODES.LSP_SERVER_UNAVAILABLE,
    `No ${languageId} language server is available for ${uri}, so "${op}" cannot be answered semantically.${startupDetail} ${hint} ` +
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

type RawGraphFactDeclaration = {
  name?: unknown;
  kind?: unknown;
  range?: unknown;
};

function isRawLspRange(value: unknown): boolean {
  if (!value || typeof value !== 'object') return false;
  const range = value as { start?: unknown; end?: unknown };
  const isPos = (p: unknown): boolean =>
    !!p &&
    typeof p === 'object' &&
    typeof (p as { line?: unknown }).line === 'number' &&
    typeof (p as { character?: unknown }).character === 'number';
  return isPos(range.start) && isPos(range.end);
}

/**
 * Fallback document-symbol source for JS/TS files oxc's full-fidelity symbol
 * extractor (`extractJsSymbols`) declines — notably Flow-typed `.js` (Flow
 * syntax like type annotations/generics can make oxc's default JS grammar
 * fail the whole-file parse, returning an empty body). `extractGraphFacts`
 * uses a more lenient extraction already proven elsewhere (localFindDeadCode)
 * to tolerate this exact file class, and its declarations already carry
 * 0-based LSP `range`s — so they slot into the same DocumentSymbol shape
 * `nativeDocumentSymbols` produces, just without a full symbol hierarchy
 * (flat top-level declarations only; `kind` is used as-is by `symbolKindName`,
 * which already passes string kinds through unchanged).
 */
export function graphFactsDocumentSymbols(
  uri: string,
  content: string
): unknown[] | null {
  if (!isNativeJsTsFile(uri)) return null;
  try {
    const json = contextUtils.extractGraphFacts(content, uri);
    if (!json) return null;
    const parsed = JSON.parse(json) as {
      declarations?: RawGraphFactDeclaration[];
    };
    const declarations = Array.isArray(parsed.declarations)
      ? parsed.declarations
      : [];
    const symbols = declarations
      .filter(
        d =>
          typeof d.name === 'string' &&
          // Drop reserved-word "symbols" the extractor mis-emitted on some Flow
          // files (e.g. `if`/`let` parsed as functions) — see jsSymbolNames.
          isValidJsSymbolName(d.name) &&
          typeof d.kind === 'string' &&
          isRawLspRange(d.range)
      )
      .map(d => ({ name: d.name, kind: d.kind, range: d.range }));
    return symbols.length > 0 ? symbols : null;
  } catch {
    return null;
  }
}
