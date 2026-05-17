/**
 * Pattern matching fallback for call hierarchy when LSP is unavailable
 */

import { getHints } from '../../hints/index.js';
import { SymbolResolver } from '../../lsp/resolver.js';
import { RipgrepMatchOnlySchema } from '../../utils/parsers/schemas.js';
import { safeExec } from '../../utils/exec/safe.js';
import { resolveRipgrepBinary } from '../../utils/exec/ripgrepBinary.js';
import type {
  CallHierarchyResult,
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
  LSPRange,
  SymbolKind,
  LSPPaginationInfo,
} from '../../lsp/types.js';
import type { LSPCallHierarchyQuery } from '@octocodeai/octocode-core';
import {
  CallSite,
  createCallHierarchyItem,
  createCallHierarchyItemFromSite,
  createRange,
  escapeRegex,
} from './callHierarchyHelpers.js';
import { TOOL_NAME } from './constants.js';

interface IncomingPatternSearchOptions {
  query: LSPCallHierarchyQuery;
  targetFilePath: string;
  workspaceRoot: string;
  targetItem: CallHierarchyItem;
  depth: number;
  callsPerPage: number;
  page: number;
  contextLines: number;
}

interface OutgoingPatternSearchOptions {
  query: LSPCallHierarchyQuery;
  filePath: string;
  content: string;
  targetItem: CallHierarchyItem;
  functionLine: number;
  depth: number;
  callsPerPage: number;
  page: number;
}

/**
 * Fallback: Use pattern matching when LSP is unavailable
 */
export async function callHierarchyWithPatternMatching(
  query: LSPCallHierarchyQuery,
  absolutePath: string,
  workspaceRoot: string,
  content: string,
  foundAtLine: number,
  _resolver: SymbolResolver
): Promise<CallHierarchyResult> {
  const lines = content.split(/\r?\n/);
  const targetItem = createCallHierarchyItem(
    query.symbolName,
    absolutePath,
    foundAtLine,
    lines,
    query.contextLines ?? 2
  );

  if (query.direction === 'incoming') {
    return await findIncomingCallsWithPatternMatching({
      query,
      targetFilePath: absolutePath,
      workspaceRoot,
      targetItem,
      depth: query.depth ?? 1,
      callsPerPage: query.callsPerPage ?? 15,
      page: query.page ?? 1,
      contextLines: query.contextLines ?? 2,
    });
  } else {
    return await findOutgoingCallsWithPatternMatching({
      query,
      filePath: absolutePath,
      content,
      targetItem,
      functionLine: foundAtLine,
      depth: query.depth ?? 1,
      callsPerPage: query.callsPerPage ?? 15,
      page: query.page ?? 1,
    });
  }
}

/**
 * Find incoming calls using ripgrep (LSP-unavailable fallback path)
 */
async function findIncomingCallsWithPatternMatching(
  options: IncomingPatternSearchOptions
): Promise<CallHierarchyResult> {
  const {
    query,
    targetFilePath,
    workspaceRoot,
    targetItem,
    depth,
    callsPerPage,
    page,
    contextLines,
  } = options;
  const symbolName = query.symbolName;

  const searchPattern = `\\b${escapeRegex(symbolName)}\\s*\\(`;

  let searchResults: CallSite[] = [];

  try {
    searchResults = await searchWithRipgrep(
      workspaceRoot,
      searchPattern,
      targetFilePath,
      contextLines
    );
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    // Error responses must match the closed `error.data` schema. Keep
    // ONLY error + hints; contextual fields (item, direction, depth,
    // lspMode) belong on hasResults/empty responses. Squeeze useful
    // context into the hints[] array so the agent still gets it.
    return {
      status: 'error',
      error: `Search failed: ${errorMessage}`,
      hints: [
        'Search for callers failed',
        `Symbol: ${symbolName} (incoming, depth=${depth})`,
        `Target: ${targetItem.uri}:${targetItem.range.start.line + 1}`,
        'Try using localSearchCode to find calls manually',
        `Pattern: ${symbolName}(`,
      ],
    };
  }

  const callSites = searchResults.filter(
    site =>
      !(
        site.filePath === targetFilePath &&
        site.lineNumber === targetItem.range.start.line + 1
      )
  );

  if (callSites.length === 0) {
    return {
      status: 'empty',
      item: targetItem,
      direction: 'incoming',
      depth,
      incomingCalls: [],
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        `No callers found for '${symbolName}'`,
        'The function may not be called directly',
        'Check if it is called via alias or dynamic invocation',
      ],
    };
  }

  const totalResults = callSites.length;
  const totalPages = Math.ceil(totalResults / callsPerPage);
  if (page > totalPages) {
    return {
      status: 'empty',
      item: targetItem,
      direction: 'incoming',
      depth,
      incomingCalls: [],
      pagination: {
        currentPage: page,
        totalPages,
        totalResults,
        hasMore: false,
        resultsPerPage: callsPerPage,
      },
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        `Requested page ${page} is outside available range (1-${totalPages}).`,
        `Use page=${totalPages} for the last available page.`,
      ],
    };
  }
  const startIndex = (page - 1) * callsPerPage;
  const paginatedSites = callSites.slice(startIndex, startIndex + callsPerPage);

  const incomingCalls: IncomingCall[] = await Promise.all(
    paginatedSites.map(async site => {
      const callerItem = await createCallHierarchyItemFromSite(
        site,
        contextLines
      );
      return {
        from: callerItem,
        fromRanges: [
          {
            start: { line: site.lineNumber - 1, character: site.column },
            end: {
              line: site.lineNumber - 1,
              character: site.column + symbolName.length,
            },
          },
        ],
      };
    })
  );

  const pagination: LSPPaginationInfo = {
    currentPage: page,
    totalPages,
    totalResults,
    hasMore: page < totalPages,
    resultsPerPage: callsPerPage,
  };

  return {
    status: 'hasResults',
    item: targetItem,
    direction: 'incoming',
    depth,
    incomingCalls,
    pagination,
    hints: [...getHints(TOOL_NAME, 'hasResults')],
  };
}

/**
 * Find outgoing calls using pattern matching (fallback)
 */
async function findOutgoingCallsWithPatternMatching(
  options: OutgoingPatternSearchOptions
): Promise<CallHierarchyResult> {
  const {
    query,
    filePath,
    content,
    targetItem,
    functionLine,
    depth,
    callsPerPage,
    page,
  } = options;

  const lines = content.split(/\r?\n/);

  const functionBody = extractFunctionBody(lines, functionLine - 1);
  if (!functionBody) {
    return {
      status: 'empty',
      item: targetItem,
      direction: 'outgoing',
      depth,
      outgoingCalls: [],
      hints: [
        'Could not extract function body',
        'The function may have unusual syntax',
        'Try using localGetFileContent to read the function manually',
      ],
    };
  }

  const callPattern = /\b([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\(/g;
  const foundCalls = new Map<string, { line: number; column: number }[]>();

  const excludePatterns = new Set([
    'if',
    'for',
    'while',
    'switch',
    'catch',
    'function',
    'return',
    'throw',
    'new',
    'typeof',
    'instanceof',
    'void',
    'delete',
    'await',
    'async',
    'class',
    'extends',
    'super',
    'this',
    'import',
    'export',
    'from',
    'as',
    'default',
    'const',
    'let',
    'var',
    'Array',
    'Object',
    'String',
    'Number',
    'Boolean',
    'Symbol',
    'BigInt',
    'Math',
    'Date',
    'JSON',
    'console',
    'Promise',
    'Error',
    'RegExp',
    'Map',
    'Set',
    'WeakMap',
    'WeakSet',
    'parseInt',
    'parseFloat',
    'isNaN',
    'isFinite',
    'encodeURI',
    'decodeURI',
    'encodeURIComponent',
    'decodeURIComponent',
    query.symbolName, // Exclude self-references for recursion
  ]);

  for (let i = 0; i < functionBody.lines.length; i++) {
    const line = functionBody.lines[i];
    if (!line) continue;

    let match;
    while ((match = callPattern.exec(line)) !== null) {
      const funcName = match[1];
      if (!funcName || excludePatterns.has(funcName)) continue;

      if (!foundCalls.has(funcName)) {
        foundCalls.set(funcName, []);
      }
      foundCalls.get(funcName)!.push({
        line: functionBody.startLine + i + 1, // 1-indexed
        column: match.index,
      });
    }
  }

  const uniqueCalls = Array.from(foundCalls.entries());

  if (uniqueCalls.length === 0) {
    return {
      status: 'empty',
      item: targetItem,
      direction: 'outgoing',
      depth,
      outgoingCalls: [],
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        `No function calls found in '${query.symbolName}'`,
        'The function may only contain primitive operations',
      ],
    };
  }

  const totalResults = uniqueCalls.length;
  const totalPages = Math.ceil(totalResults / callsPerPage);
  if (page > totalPages) {
    return {
      status: 'empty',
      item: targetItem,
      direction: 'outgoing',
      depth,
      outgoingCalls: [],
      pagination: {
        currentPage: page,
        totalPages,
        totalResults,
        hasMore: false,
        resultsPerPage: callsPerPage,
      },
      hints: [
        ...getHints(TOOL_NAME, 'empty'),
        `Requested page ${page} is outside available range (1-${totalPages}).`,
        `Use page=${totalPages} for the last available page.`,
      ],
    };
  }
  const startIndex = (page - 1) * callsPerPage;
  const paginatedCalls = uniqueCalls.slice(
    startIndex,
    startIndex + callsPerPage
  );

  const outgoingCalls: OutgoingCall[] = paginatedCalls.map(
    ([funcName, locations]) => {
      const firstLoc = locations[0]!;
      const calleeItem: CallHierarchyItem = {
        name: funcName,
        kind: 'function' as SymbolKind,
        uri: filePath,
        range: createRange(firstLoc.line - 1, firstLoc.column, funcName.length),
      };

      const fromRanges: LSPRange[] = locations.map(loc =>
        createRange(loc.line - 1, loc.column, funcName.length)
      );

      return {
        to: calleeItem,
        fromRanges,
      };
    }
  );

  const pagination: LSPPaginationInfo = {
    currentPage: page,
    totalPages,
    totalResults,
    hasMore: page < totalPages,
    resultsPerPage: callsPerPage,
  };

  return {
    status: 'hasResults',
    item: targetItem,
    direction: 'outgoing',
    depth,
    outgoingCalls,
    pagination,
    hints: [
      ...getHints(TOOL_NAME, 'hasResults'),
      'Use lspGotoDefinition to find where each callee is defined',
    ],
  };
}

/**
 * Search for pattern using ripgrep
 */
async function searchWithRipgrep(
  workspaceRoot: string,
  pattern: string,
  _excludeFile: string,
  contextLines: number
): Promise<CallSite[]> {
  // Flags must stay inside the security validator's RG allow-list
  // (octocode-security-utils/commandValidator):
  //   - `-n` (not `--line-number`)
  //   - pattern as positional (`-e` is not allow-listed)
  //   - `--glob` to filter TS/JS family (`--type-add` is not allow-listed,
  //     and default `--type ts` doesn't cover all variants on older rg).
  const args = [
    '--json',
    '-n',
    '--column',
    '--glob',
    '*.{ts,tsx,js,jsx,mjs,cjs}',
    '-C',
    String(contextLines),
    '--',
    pattern,
    workspaceRoot,
  ];

  const result = await safeExec(resolveRipgrepBinary(), args, {
    cwd: workspaceRoot,
    timeout: 30000,
  });

  if (!result.success && result.code !== 1) {
    throw new Error(result.stderr || 'ripgrep search failed');
  }

  return parseRipgrepJsonOutput(result.stdout);
}

/**
 * Parse ripgrep JSON output
 * @internal Exported for testing
 */
export function parseRipgrepJsonOutput(output: string): CallSite[] {
  const results: CallSite[] = [];
  const lines = output.split('\n').filter(line => line.trim());

  for (const line of lines) {
    try {
      const raw = JSON.parse(line);
      const validation = RipgrepMatchOnlySchema.safeParse(raw);
      if (!validation.success) continue;
      const data = validation.data.data;
      results.push({
        filePath: data.path.text,
        lineNumber: data.line_number,
        column: data.submatches?.[0]?.start || 0,
        lineContent: data.lines.text,
      });
    } catch {
      // Malformed JSON line from ripgrep; skip this line.
    }
  }

  return results;
}

/**
 * Extract function body starting from a line
 * @internal Exported for testing
 */
export function extractFunctionBody(
  lines: string[],
  startLineIndex: number
): { lines: string[]; startLine: number; endLine: number } | null {
  let braceCount = 0;
  let foundStart = false;
  let bodyStartLine = startLineIndex;
  const bodyLines: string[] = [];

  for (
    let i = startLineIndex;
    i < Math.min(lines.length, startLineIndex + 5);
    i++
  ) {
    const line = lines[i];
    if (!line) continue;

    const braceIndex = line.indexOf('{');
    if (braceIndex !== -1) {
      foundStart = true;
      bodyStartLine = i;
      braceCount = 1;

      for (let j = braceIndex + 1; j < line.length; j++) {
        if (line[j] === '{') braceCount++;
        if (line[j] === '}') braceCount--;
      }

      bodyLines.push(line.slice(braceIndex + 1));
      break;
    }
  }

  if (!foundStart) return null;

  for (let i = bodyStartLine + 1; i < lines.length && braceCount > 0; i++) {
    const line = lines[i];
    if (!line) {
      bodyLines.push('');
      continue;
    }

    for (const char of line) {
      if (char === '{') braceCount++;
      if (char === '}') braceCount--;
    }

    if (braceCount > 0) {
      bodyLines.push(line);
    } else {
      const lastBraceIndex = line.lastIndexOf('}');
      if (lastBraceIndex > 0) {
        bodyLines.push(line.slice(0, lastBraceIndex));
      }
    }
  }

  return {
    lines: bodyLines,
    startLine: bodyStartLine,
    endLine: bodyStartLine + bodyLines.length,
  };
}
