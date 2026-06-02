/**
 * LSP Go To Definition execution logic
 * @module tools/lsp_goto_definition/execution
 */

import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { readFile } from 'fs/promises';
import { dirname, resolve as resolvePath } from 'path';

import type { z } from 'zod/v4';
import type { LSPGotoDefinitionQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPGotoDefinitionQuery = z.infer<
  typeof LSPGotoDefinitionQuerySchema
>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';

/** Advisory hints lspGotoDefinition emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryGotoDefinitionHint = makeAdvisoryPredicate([
  'multiple definitions',
  'dynamic import',
  'fallback',
  'overload',
  're-export',
]);
import type { WithOptionalMeta } from '../../types/execution.js';

type LSPGotoDefinitionQuery = WithVerbosity<
  WithOptionalMeta<UpstreamLSPGotoDefinitionQuery>
> & {
  orderHint?: number;
};
import { SymbolResolver, SymbolResolutionError } from '../../lsp/resolver.js';
import {
  acquirePooledClient,
  isLanguageServerAvailable,
  LSP_UNAVAILABLE_HINT,
} from '../../lsp/manager.js';
import type {
  GotoDefinitionResult,
  CodeSnippet,
  ExactPosition,
} from '../../lsp/types.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { getHints } from '../../hints/index.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import { applyOutputSizeLimit } from '../../utils/pagination/outputSizeLimit.js';
import { serializeForPagination } from '../../utils/pagination/core.js';
import { safeReadFile } from '../../lsp/validation.js';
import { resolveWorkspaceRootForFile } from '../../lsp/workspaceRoot.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../utils/response/charSavings.js';

export const TOOL_NAME = TOOL_NAMES.LSP_GOTO_DEFINITION;

/**
 * Execute bulk goto definition operation.
 * Wraps gotoDefinition with bulk operation handling for multiple queries.
 */
export async function executeGotoDefinition(
  args: ToolExecutionArgs<LSPGotoDefinitionQuery>
): Promise<CallToolResult> {
  const { queries, responseCharOffset, responseCharLength } = args;

  return executeBulkOperation(
    queries || [],
    async (query: LSPGotoDefinitionQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAME,
        query,
        contextMessage: 'lspGotoDefinition execution failed',
        execute: async () =>
          attachDefinitionEvidence(await gotoDefinition(query)),
      }),
    {
      toolName: TOOL_NAME,
      responseCharOffset,
      responseCharLength,
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}

/**
 * Attach cross-tool evidence so the bulk runner can lift it to the response
 * envelope. Confidence is `high` for LSP semantic results and `low` for the
 * text-pattern fallback; the goto-definition response either resolves
 * fully or not at all, so `complete` follows `answerReady`.
 */
function attachDefinitionEvidence(
  result: GotoDefinitionResult
): GotoDefinitionResult {
  // Only annotate well-shaped LSP results — skip raw error envelopes so we
  // don't add an evidence block to shapes that tests assert verbatim.
  // Lean contract: ABSENT status ≡ success; only 'empty' / 'error' emit.
  const status = (result as { status?: string }).status;
  if (status !== undefined && status !== 'empty') return result;
  const hasResults = status === undefined;
  // lspMode absent ≡ semantic; 'fallback' is the only emitted marker.
  const mode = (result as { lspMode?: 'semantic' | 'fallback' }).lspMode;
  const isSemantic = mode === undefined || mode === 'semantic';
  const evidence = {
    kind: 'definition' as const,
    answerReady: hasResults,
    complete: hasResults,
    confidence: isSemantic ? ('high' as const) : ('low' as const),
    ...(mode === 'fallback'
      ? {
          reason:
            'Resolved via text fallback; may point at an import line rather than the source definition.',
        }
      : {}),
  };
  // Mutate in place so any non-enumerable raw-chars symbol attached
  // upstream (see attachRawResponseChars) survives.
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}

/**
 * Execute goto definition for a single query
 */
async function gotoDefinition(
  query: LSPGotoDefinitionQuery
): Promise<GotoDefinitionResult> {
  try {
    const pathValidation = validateToolPath(
      { ...query, path: query.uri },
      TOOL_NAME
    );
    if (!pathValidation.isValid) {
      return pathValidation.errorResult as GotoDefinitionResult;
    }

    const absolutePath = pathValidation.sanitizedPath!;

    let content: string;
    try {
      content = await readFile(absolutePath, 'utf-8');
    } catch (error) {
      return createErrorResult(error, query, {
        toolName: TOOL_NAME,
        extra: { resolvedPath: absolutePath },
        customHints: [`Could not read file: ${query.uri}`],
      }) as GotoDefinitionResult;
    }

    const symbolName = query.symbolName!;
    const lineHint = query.lineHint!;
    const resolver = new SymbolResolver({ lineSearchRadius: 5 });
    let resolvedSymbol;
    try {
      resolvedSymbol = resolver.resolvePositionFromContent(content, {
        symbolName,
        lineHint,
        orderHint: query.orderHint ?? 0,
      });
    } catch (error) {
      if (error instanceof SymbolResolutionError) {
        return attachRawResponseChars(
          {
            status: 'empty',
            error: error.message,
            errorType: 'symbol_not_found',
            errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
            searchRadius: error.searchRadius,
            hints: [
              ...getHints(TOOL_NAME, 'empty'),
              `Symbol "${symbolName}" not found at or near line ${lineHint}`,
              `Searched lines ${Math.max(1, lineHint - error.searchRadius)} to ${lineHint + error.searchRadius}`,
              'Verify the exact symbol name (case-sensitive, no partial matches)',
              'Adjust lineHint if the symbol moved due to code changes',
              query.orderHint && query.orderHint > 0
                ? `orderHint=${query.orderHint} targets the ${query.orderHint + 1}th code occurrence on the exact line`
                : undefined,
              query.orderHint && query.orderHint > 0
                ? 'orderHint skips string/comment text matches and does not apply to nearby fallback lines'
                : undefined,
              query.orderHint && query.orderHint > 0
                ? 'Try orderHint=0 if the symbol appears once in code on that line'
                : undefined,
            ].filter(Boolean) as string[],
          },
          content.length
        );
      }
      throw error;
    }

    const workspaceRoot = await resolveWorkspaceRootForFile(absolutePath);
    const lspAvailable = await isLanguageServerAvailable(
      absolutePath,
      workspaceRoot
    );

    let semanticFallbackHint: string | undefined;
    if (lspAvailable) {
      try {
        const result = await gotoDefinitionWithLSP(
          absolutePath,
          workspaceRoot,
          resolvedSymbol.position,
          query,
          content
        );
        if (result) {
          // Semantic path: omit lspMode (absent ≡ semantic). Fallback path
          // below explicitly sets lspMode='fallback'.
          const semanticResult = result;
          return attachRawResponseChars(
            applyGotoDefinitionVerbosity(
              applyGotoDefinitionOutputLimit(semanticResult, query),
              query
            ),
            content.length + countSerializedChars(semanticResult)
          );
        }
        semanticFallbackHint =
          'LSP semantic lookup returned no result; using text fallback';
      } catch {
        semanticFallbackHint =
          'LSP semantic lookup failed; using text fallback';
      }
    }

    const fallback = createFallbackResult(
      query,
      absolutePath,
      content,
      resolver,
      resolvedSymbol,
      { lspUnavailable: !lspAvailable, semanticFallbackHint }
    );
    // `lspMode` is only valid on hasResults/empty per the published
    // output schema. Skip it on error so MCP schema validation passes.
    const tagged: GotoDefinitionResult =
      fallback.status === 'error'
        ? fallback
        : { ...fallback, lspMode: 'fallback' };
    return attachRawResponseChars(
      applyGotoDefinitionVerbosity(
        applyGotoDefinitionOutputLimit(tagged, query),
        query
      ),
      content.length + countSerializedChars(tagged)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as GotoDefinitionResult;
  }
}

/**
 * Detect whether a line of code is an import or re-export statement.
 * Used to determine if a goto-definition result resolved to an import
 * rather than the actual source definition.
 *
 * Covers TypeScript/JavaScript patterns:
 * - import { Foo } from './module'
 * - import Foo from './module'
 * - import * as Foo from './module'
 * - export { Foo } from './module'
 * - export * from './module'
 * - export { default as Foo } from './module'
 *
 * @internal Exported for testing
 */
export function isImportOrReExport(lineContent: string): boolean {
  const trimmed = lineContent.trim();
  return /^(?:import|export)\s+.*\bfrom\b\s+['"]/.test(trimmed);
}

/**
 * Detect whether a line contains a dynamic import expression (`import('...')`).
 *
 * Covers patterns:
 * - const { foo } = await import('./module')
 * - import('./module').then(...)
 * - const mod = import("./module")
 *
 * Uses a negative lookbehind to avoid matching identifiers that end with
 * "import" (e.g. `reimport`, `importModule`).
 *
 * @internal Exported for testing
 */
export function isDynamicImport(lineContent: string): boolean {
  return /(?<![.\w])import\s*\(\s*['"]/.test(lineContent);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Resolve the best cursor character for a second-hop goto-definition call
 * on import/re-export lines. Prefers the queried symbol token position
 * before the "from" clause to avoid matching module path text.
 */
function resolveImportSymbolCharacter(
  lineContent: string,
  symbolName: string,
  fallbackCharacter: number
): number {
  if (!lineContent || !symbolName) return fallbackCharacter;

  const fromMatch = /\bfrom\b/.exec(lineContent);
  const searchScope = fromMatch
    ? lineContent.slice(0, fromMatch.index)
    : lineContent;
  const symbolRegex = new RegExp(`\\b${escapeRegExp(symbolName)}\\b`);
  const match = symbolRegex.exec(searchScope);

  return match ? match.index : fallbackCharacter;
}

/**
 * Use LSP client to find definition, with automatic import chaining.
 * If the LSP resolves to an import/re-export in the same file,
 * performs one additional hop to follow the import to the source definition.
 */
/** A pooled LSP client (non-null result of acquirePooledClient). */
type PooledLspClient = NonNullable<
  Awaited<ReturnType<typeof acquirePooledClient>>
>;

/** Error result when the language server lacks definitionProvider. */
function buildUnsupportedCapabilityResult(): GotoDefinitionResult {
  return {
    status: 'error',
    error: 'Language server does not support goto definition',
    errorType: 'unknown',
    errorCode: LSP_ERROR_CODES.LSP_CAPABILITY_UNSUPPORTED,
    hints: [
      ...getHints(TOOL_NAME, 'error'),
      'The active language server does not advertise definitionProvider.',
      'Try localSearchCode as a text fallback.',
      'Check LSP server configuration for this language.',
    ],
  };
}

/**
 * When the LSP returns no locations, try resolving a dynamic-import binding
 * via the module path (`.js` → `.ts`). Returns a ready result on success,
 * or null to let the caller emit the standard "not found" response.
 */
async function resolveDynamicImportFallback(
  content: string,
  position: ExactPosition,
  filePath: string,
  symbolName: string,
  query: LSPGotoDefinitionQuery
): Promise<GotoDefinitionResult | null> {
  const lines = content.split(/\r?\n/);
  const targetLine = lines[position.line];
  if (!targetLine || !isDynamicImport(targetLine)) return null;

  const manualLocation = await resolveDefinitionViaModulePath(
    targetLine,
    filePath,
    symbolName
  );
  if (!manualLocation) return null;

  return applyGotoDefinitionOutputLimit(
    {
      locations: [manualLocation],
      resolvedPosition: position,
      searchRadius: 5,
      hints: ['Resolved via dynamic import module path (.js → .ts)'],
    },
    query
  );
}

/**
 * When the single LSP location is itself an import/re-export in the same
 * file, follow it to the real source — first via a chained gotoDefinition,
 * then via manual module-path resolution. Returns the (possibly rewritten)
 * locations and whether an import was followed.
 */
async function followImportToSource(
  client: PooledLspClient,
  locations: CodeSnippet[],
  filePath: string,
  symbolName: string
): Promise<{ locations: CodeSnippet[]; followedImport: boolean }> {
  if (locations.length !== 1) return { locations, followedImport: false };

  const loc = locations[0]!;
  if (loc.uri !== filePath) return { locations, followedImport: false };

  try {
    const locContent = await safeReadFile(loc.uri);
    if (!locContent) throw new Error('Cannot read file');
    const lines = locContent.split(/\r?\n/);
    const targetLine = lines[loc.range.start.line];

    if (
      !targetLine ||
      !(isImportOrReExport(targetLine) || isDynamicImport(targetLine))
    ) {
      return { locations, followedImport: false };
    }

    const importPosition: ExactPosition = {
      line: loc.range.start.line,
      character: resolveImportSymbolCharacter(
        targetLine,
        symbolName,
        loc.range.start.character
      ),
    };

    const chainedLocations = await client.gotoDefinition(
      loc.uri,
      importPosition
    );
    if (chainedLocations && chainedLocations.length > 0) {
      const resolvedToDifferentFile = chainedLocations.some(
        cl => cl.uri !== filePath
      );
      if (resolvedToDifferentFile) {
        return {
          locations: chainedLocations.filter(cl => cl.uri !== filePath),
          followedImport: true,
        };
      }
    }

    const manualLocation = await resolveDefinitionViaModulePath(
      targetLine,
      loc.uri,
      symbolName
    );
    if (manualLocation) {
      return { locations: [manualLocation], followedImport: true };
    }
  } catch {
    // Import-chain or module-path resolution failed; keep original LSP locations.
  }

  return { locations, followedImport: false };
}

/**
 * Attach a line-numbered context snippet to a definition location. On any
 * read failure the raw location is returned unchanged.
 */
async function enhanceLocationWithSnippet(
  loc: CodeSnippet,
  contextLines: number
): Promise<CodeSnippet> {
  try {
    const locContent = await safeReadFile(loc.uri);
    if (!locContent) {
      return loc;
    }
    const lines = locContent.split(/\r?\n/);
    const startLine = Math.max(0, loc.range.start.line - contextLines);
    const endLine = Math.min(
      lines.length - 1,
      loc.range.end.line + contextLines
    );

    const snippetLines = lines.slice(startLine, endLine + 1);
    const numberedContent = snippetLines
      .map((line, i) => {
        const lineNum = startLine + i + 1;
        const isTarget =
          lineNum > loc.range.start.line && lineNum <= loc.range.end.line + 1;
        const marker = isTarget ? '>' : ' ';
        return `${marker}${String(lineNum).padStart(4, ' ')}| ${line}`;
      })
      .join('\n');

    return {
      ...loc,
      content: numberedContent,
    };
  } catch {
    // Snippet enhancement failed; keep raw LSP location.
    return loc;
  }
}

async function gotoDefinitionWithLSP(
  filePath: string,
  workspaceRoot: string,
  _position: ExactPosition,
  query: LSPGotoDefinitionQuery,
  _content: string
): Promise<GotoDefinitionResult | null> {
  // Pooled client: the pool owns its lifecycle, so we MUST NOT stop() it
  // here. Idle eviction tears it down later (see lsp/lspClientPool.ts).
  const client = await acquirePooledClient(workspaceRoot, filePath);
  if (!client) return null;

  if (client.hasCapability && !client.hasCapability('definitionProvider')) {
    return buildUnsupportedCapabilityResult();
  }

  const symbolName = query.symbolName!;
  let locations = await client.gotoDefinition(filePath, _position);

  if (!locations || locations.length === 0) {
    const fallback = await resolveDynamicImportFallback(
      _content,
      _position,
      filePath,
      symbolName,
      query
    );
    if (fallback) return fallback;

    return {
      status: 'empty',
      error: 'No definition found by language server',
      errorType: 'symbol_not_found',
      errorCode: LSP_ERROR_CODES.SYMBOL_NOT_FOUND,
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        'Language server could not find definition',
        'Symbol may be a built-in or from external library',
        'Try packageSearch to find library source code',
      ],
    };
  }

  const chained = await followImportToSource(
    client,
    locations,
    filePath,
    symbolName
  );
  locations = chained.locations;
  const followedImport = chained.followedImport;

  const contextLines = query.contextLines ?? 5;
  const enhancedLocations = await Promise.all(
    locations.map(loc => enhanceLocationWithSnippet(loc, contextLines))
  );

  const strippedLocations = enhancedLocations.map(
    ({ displayRange: _, ...rest }) => rest
  );
  return {
    locations: strippedLocations,
    resolvedPosition: _position,
    searchRadius: 5,
    hints: [
      followedImport ? 'Followed import chain to source definition' : undefined,
      locations.length > 1
        ? 'Multiple definitions - check overloads or re-exports'
        : undefined,
    ].filter(Boolean) as string[],
  };
}

/**
 * Fallback for TypeScript ESM projects that use `.js` extension imports.
 *
 * When TypeScript LSP cannot follow `import { X } from './y.js'` to `y.ts`
 * (because the language server only sees the local import binding as the
 * "definition"), we resolve the module path manually:
 *  1. Parse the `from '...'` clause in the import line.
 *  2. Map `.js` → `.ts` (TypeScript ESM convention).
 *  3. Text-search the target file for the first `export` line that
 *     contains the symbol name.
 *
 * Returns a minimal Location object compatible with the locations array, or
 * null when the module cannot be resolved or the symbol is not found.
 */
async function resolveDefinitionViaModulePath(
  importLine: string,
  sourceFileUri: string,
  symbolName: string
): Promise<CodeSnippet | null> {
  // Try static import: from '...'
  const fromMatch = /\bfrom\s+['"](.+?)['"]\s*;?\s*$/.exec(importLine);
  let modulePath: string | null = fromMatch?.[1] ?? null;

  // Try dynamic import: import('...')
  if (!modulePath) {
    const dynamicMatch = /\bimport\s*\(\s*['"](.+?)['"]\s*\)/.exec(importLine);
    modulePath = dynamicMatch?.[1] ?? null;
  }

  if (!modulePath) return null;
  if (!modulePath.startsWith('.')) return null;

  const sourceDir = dirname(sourceFileUri);
  let resolvedPath = resolvePath(sourceDir, modulePath);

  if (resolvedPath.endsWith('.js')) {
    resolvedPath = resolvedPath.replace(/\.js$/, '.ts');
  }

  const content = await safeReadFile(resolvedPath);
  if (!content) return null;

  const lines = content.split(/\r?\n/);
  const symbolWordBoundaryRe = new RegExp(`\\b${escapeRegExp(symbolName)}\\b`);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (/^\s*export\b/.test(line) && symbolWordBoundaryRe.test(line)) {
      const charIdx = line.indexOf(symbolName);
      return {
        uri: resolvedPath,
        content: '',
        range: {
          start: { line: i, character: charIdx },
          end: { line: i, character: charIdx + symbolName.length },
        },
      };
    }
  }

  return null;
}

/**
 * Create fallback result when LSP is not available
 */
function createFallbackResult(
  query: LSPGotoDefinitionQuery,
  absolutePath: string,
  content: string,
  resolver: SymbolResolver,
  resolvedSymbol: { position: ExactPosition; foundAtLine: number },
  options: { lspUnavailable?: boolean; semanticFallbackHint?: string } = {}
): GotoDefinitionResult {
  const symbolName = query.symbolName!;
  const contextLines = query.contextLines ?? 5;
  const context = resolver.extractContext(
    content,
    resolvedSymbol.foundAtLine,
    contextLines
  );

  const numberedContent = addLineNumbers(
    context.content,
    context.startLine,
    resolvedSymbol.foundAtLine
  );

  const codeSnippet: CodeSnippet = {
    uri: absolutePath,
    range: {
      start: resolvedSymbol.position,
      end: {
        line: resolvedSymbol.position.line,
        character: resolvedSymbol.position.character + symbolName.length,
      },
    },
    content: numberedContent,
  };

  return {
    locations: [codeSnippet],
    resolvedPosition: resolvedSymbol.position,
    searchRadius: 5,
    hints: [
      options.lspUnavailable ? LSP_UNAVAILABLE_HINT : undefined,
      options.semanticFallbackHint,
      resolvedSymbol.foundAtLine !== query.lineHint
        ? `Symbol found at line ${resolvedSymbol.foundAtLine} (hint was ${query.lineHint})`
        : undefined,
    ].filter(Boolean) as string[],
  };
}

/**
 * When `verbosity:"concise"` is requested, collapse each location
 * to a `file:line:col` string (drop `content` snippets) and emit a single
 * summary hint. Omitted / `"basic"` / `"compact"` behave identically to today.
 *
 * Exported for direct unit testing in `tests/scheme/verbosity_concise.test.ts`.
 */
export function applyGotoDefinitionVerbosity(
  result: GotoDefinitionResult,
  query: LSPGotoDefinitionQuery
): GotoDefinitionResult {
  if (isConcise(query.verbosity)) {
    if (result.status !== undefined) return result;
    const refs = (result.locations ?? []).map(loc => {
      const line = loc.range?.start?.line ?? 0;
      const col = loc.range?.start?.character ?? 0;
      return `${loc.uri}:${line + 1}:${col + 1}`;
    });
    const top = refs[0] ?? '';
    const summary = `${refs.length} definition(s)${top ? ` (top: ${top})` : ''}`;
    return {
      ...result,
      locations: (result.locations ?? []).map(loc => ({
        uri: loc.uri,
        range: loc.range,
        content: '',
      })),
      hints: [summary],
    };
  }
  if (isCompact(query.verbosity)) {
    return {
      ...result,
      hints: compactTrimHints(result.hints, isAdvisoryGotoDefinitionHint, 2),
    };
  }
  return result;
}

/**
 * Apply output size limits with charOffset/charLength pagination.
 * Follows the same pattern used by lspCallHierarchy.
 */
function applyGotoDefinitionOutputLimit(
  result: GotoDefinitionResult,
  query: LSPGotoDefinitionQuery
): GotoDefinitionResult {
  if (result.status !== undefined) return result;

  const serialized = serializeForPagination(result, true);
  const sizeLimitResult = applyOutputSizeLimit(serialized, {
    charOffset: query.charOffset,
    charLength: query.charLength,
  });

  if (!sizeLimitResult.wasLimited || !sizeLimitResult.pagination) return result;

  const { pagination } = sizeLimitResult;
  return {
    ...result,
    outputPagination: {
      charOffset: pagination.charOffset!,
      charLength: pagination.charLength!,
      totalChars: pagination.totalChars!,
      hasMore: pagination.hasMore,
      currentPage: pagination.currentPage,
      totalPages: pagination.totalPages,
    },
    hints: [
      ...(result.hints || []),
      ...sizeLimitResult.warnings,
      ...sizeLimitResult.paginationHints,
    ],
  };
}

/**
 * Add line numbers to code content, highlighting the target line
 * @internal Exported for testing
 */
export function addLineNumbers(
  content: string,
  startLine: number,
  targetLine: number
): string {
  const lines = content.split('\n');
  const maxLineNum = startLine + lines.length - 1;
  const lineNumWidth = String(maxLineNum).length;

  return lines
    .map((line, index) => {
      const lineNum = startLine + index;
      const paddedNum = String(lineNum).padStart(lineNumWidth, ' ');
      const marker = lineNum === targetLine ? '>' : ' ';
      return `${marker}${paddedNum}| ${line}`;
    })
    .join('\n');
}
