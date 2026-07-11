import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unavailableHintFor } from '@octocodeai/octocode-engine/lsp/manager';
import { detectLanguageId } from '@octocodeai/octocode-engine/lsp/config';
import { ToolError } from '../../../../errors/ToolError.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../../../errors/localToolErrors.js';
import { contextUtils } from '../../../../utils/contextUtils.js';
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
