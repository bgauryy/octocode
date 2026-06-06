import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { readFile } from 'fs/promises';

import type { z } from 'zod';
import type { LSPGotoDefinitionQuerySchema } from '@octocodeai/octocode-core/schemas';

type UpstreamLSPGotoDefinitionQuery = z.infer<
  typeof LSPGotoDefinitionQuerySchema
>;
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import { isVerbose } from '../../scheme/verbosity.js';
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
import { safeReadFile } from '../../lsp/validation.js';
import { resolveWorkspaceRootForFile } from '../../lsp/workspaceRoot.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { LSP_ERROR_CODES } from '../../lsp/lspErrorCodes.js';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../utils/response/charSavings.js';

export const TOOL_NAME = TOOL_NAMES.LSP_GOTO_DEFINITION;

export async function executeGotoDefinition(
  args: ToolExecutionArgs<LSPGotoDefinitionQuery>
): Promise<CallToolResult> {
  const { queries } = args;

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
      peerHints: true,
      peerEvidence: true,
      minQueryTimeoutMs: 30_000,
    }
  );
}

function attachDefinitionEvidence(
  result: GotoDefinitionResult
): GotoDefinitionResult {
  const status = (result as { status?: string }).status;
  if (status !== undefined && status !== 'empty') return result;
  const hasResults = status === undefined;
  const evidence = {
    kind: 'definition' as const,
    answerReady: hasResults,
    complete: hasResults,
    confidence: 'high' as const,
  };
  (result as Record<string, unknown>).evidence = evidence;
  return result;
}

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
              `Symbol "${symbolName}" not found at or near line ${lineHint} — lineHint is likely stale (file changed since the line was recorded).`,
              `Searched lines ${Math.max(1, lineHint - error.searchRadius)} to ${lineHint + error.searchRadius}`,
              'Re-anchor: run localSearchCode with the exact symbol name to get the current line number, then retry with that lineHint.',
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

    if (!lspAvailable) {
      return attachRawResponseChars(
        buildLspUnavailableResult(),
        content.length
      );
    }

    let result: GotoDefinitionResult | null = null;
    try {
      result = await gotoDefinitionWithLSP(
        absolutePath,
        workspaceRoot,
        resolvedSymbol.position,
        query,
        content
      );
    } catch {
      result = null;
    }

    if (!result) {
      return attachRawResponseChars(
        buildLspUnavailableResult(true),
        content.length
      );
    }

    return attachRawResponseChars(
      applyGotoDefinitionVerbosity(result, query),
      content.length + countSerializedChars(result)
    );
  } catch (error) {
    return createErrorResult(error, query, {
      toolName: TOOL_NAME,
    }) as GotoDefinitionResult;
  }
}

export function isImportOrReExport(lineContent: string): boolean {
  const trimmed = lineContent.trim();
  return /^(?:import|export)\s+.*\bfrom\b\s+['"]/.test(trimmed);
}

export function isDynamicImport(lineContent: string): boolean {
  return /(?<![.\w])import\s*\(\s*['"]/.test(lineContent);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

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

type PooledLspClient = NonNullable<
  Awaited<ReturnType<typeof acquirePooledClient>>
>;

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
  } catch {
    void 0;
  }

  return { locations, followedImport: false };
}

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
  const client = await acquirePooledClient(workspaceRoot, filePath);
  if (!client) return null;

  if (client.hasCapability && !client.hasCapability('definitionProvider')) {
    return buildUnsupportedCapabilityResult();
  }

  const symbolName = query.symbolName!;
  let locations = await client.gotoDefinition(filePath, _position);

  if (!locations || locations.length === 0) {
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
      'Definition found — use lspFindReferences with the same symbolName+lineHint to find all usages, or lspCallHierarchy to trace call flow.',
    ].filter(Boolean) as string[],
  };
}

function buildLspUnavailableResult(lspFailed = false): GotoDefinitionResult {
  return {
    status: 'empty',
    errorType: 'unknown',
    errorCode: lspFailed
      ? LSP_ERROR_CODES.LSP_EMPTY
      : LSP_ERROR_CODES.LSP_NOT_INSTALLED,
    searchRadius: 5,
    hints: [
      ...getHints(TOOL_NAME, 'empty'),
      lspFailed
        ? 'The language server returned no definition for this symbol.'
        : LSP_UNAVAILABLE_HINT,
      'Use localSearchCode to locate the declaration by text, or packageSearch for external library source.',
    ],
  };
}

export function applyGotoDefinitionVerbosity(
  result: GotoDefinitionResult,
  query: LSPGotoDefinitionQuery
): GotoDefinitionResult {
  if (isVerbose(query) || result.status !== undefined) return result;
  if (!('lspMode' in (result as object))) return result;
  const { lspMode: _lm, ...rest } = result as typeof result & {
    lspMode?: unknown;
  };
  void _lm;
  return rest as GotoDefinitionResult;
}

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
