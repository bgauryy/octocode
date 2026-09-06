import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import { MAX_MATCH_CONTENT_LENGTH, MAX_PAGE_NUMBER } from '../../../config.js';
import { RESOURCE_LIMITS } from '../../../utils/core/constants.js';

import type { RipgrepQuery } from '../scheme.js';
import type { LocalSearchEngine } from './buildResult.js';

const FETCH_CONTEXT_LINES = 8;
const RESERVED_SYMBOL_WORDS = new Set([
  'async',
  'await',
  'break',
  'case',
  'catch',
  'class',
  'const',
  'def',
  'do',
  'else',
  'enum',
  'export',
  'for',
  'function',
  'if',
  'import',
  'interface',
  'let',
  'match',
  'return',
  'struct',
  'switch',
  'type',
  'var',
  'while',
  // Literals and contextual keywords that are never resolvable symbols.
  'true',
  'false',
  'null',
  'undefined',
  'NaN',
  'Infinity',
  'this',
  'super',
]);

// A candidate is only an LSP-resolvable symbol when its *entire* value is one
// bare identifier — anchored, no surrounding regex/punctuation/whitespace.
const BARE_IDENTIFIER = /^[A-Za-z_$][A-Za-z0-9_$]*$/;

type NextToolName = 'localGetFileContent' | 'lspGetSemantics' | 'local.text';

type NextConfidence = 'exact' | 'low';

type SearchNextCall = {
  tool: NextToolName;
  query: Record<string, unknown>;
  why: string;
  confidence?: NextConfidence;
};

export type SearchNextMap = {
  fetch?: SearchNextCall;
  lspDefinition?: SearchNextCall;
  lspReferences?: SearchNextCall;
  nextPage?: SearchNextCall;
  nextMatchPage?: SearchNextCall;
  expandScan?: SearchNextCall;
  expandCaptures?: SearchNextCall;
};

type FlowMatch = {
  line?: number;
  endLine?: number;
  value?: string;
  metavars?: Record<string, string[]>;
  metavarRanges?: Record<string, Array<{ line: number }>>;
  capturesTruncated?: boolean;
};

type FlowFile = {
  path?: string;
  matches?: FlowMatch[];
  pagination?: { hasMore?: boolean };
};

export function buildSearchNextMap(
  files: LocalSearchCodeFile[],
  query: RipgrepQuery,
  searchEngine: LocalSearchEngine,
  options: {
    isFileListMode: boolean;
    currentPage: number;
    totalFilePages: number;
    matchPage: number;
    matchesPerPage: number;
    hasFileWithMoreMatches: boolean;
    scanCapReached?: boolean;
  }
): SearchNextMap {
  const firstFile = (files as FlowFile[]).find(file => file.path);
  const firstMatch = firstFile?.matches?.find(match => match.line);
  const next: SearchNextMap = {};

  const currentMaxFiles = query.maxFiles ?? RESOURCE_LIMITS.MAX_FILES_DEFAULT;
  if (options.scanCapReached && currentMaxFiles < MAX_MATCH_CONTENT_LENGTH) {
    next.expandScan = {
      tool: 'local.text',
      query: withoutUndefined({
        ...query,
        maxFiles: Math.min(
          MAX_MATCH_CONTENT_LENGTH,
          Math.max(currentMaxFiles + 1, currentMaxFiles * 2)
        ),
        page: 1,
      }),
      why: 'Re-run with a larger file-scan bound because this search is partial.',
      confidence: 'exact',
    };
  }

  if (
    searchEngine === 'structural' &&
    (files as FlowFile[]).some(file =>
      file.matches?.some(match => match.capturesTruncated === true)
    )
  ) {
    next.expandCaptures = {
      tool: 'local.text',
      query: withoutUndefined({ ...query, captureText: true }),
      why: 'Replay this structural page with verbatim capture text because the default capture budget omitted or shortened capture payloads.',
      confidence: 'exact',
    };
  }

  if (firstFile?.path) {
    // ONE fetch continuation instead of exact/standard/symbols triplets —
    // the variants only differed by `minify`, while each repeated the full
    // absolute path (the top token cost of small search responses). The why
    // documents the knob so agents can still pick a different view.
    if (firstMatch?.line) {
      const range = lineRangeAroundMatch(firstMatch);
      next.fetch = {
        tool: 'localGetFileContent',
        query: withoutUndefined({
          path: firstFile.path,
          startLine: range.startLine,
          endLine: range.endLine,
          minify: 'none',
        }),
        why: 'Read exact source around the first match (set minify:"standard" for a token-lean slice, or drop the line range with minify:"symbols" for a skeleton).',
        confidence: 'exact',
      };
    } else {
      next.fetch = {
        tool: 'localGetFileContent',
        query: { path: firstFile.path, minify: 'standard' },
        why: 'Read the first matched file (minify:"symbols" gives a skeleton for orientation; minify:"none" gives exact bytes).',
        confidence: options.isFileListMode ? 'low' : 'exact',
      };
    }

    const inferred = inferLspSymbolName(firstMatch, query, searchEngine);
    if (inferred && firstMatch?.line) {
      const lspBase = {
        uri: firstFile.path,
        symbolName: inferred.symbol,
        lineHint: inferred.line ?? firstMatch.line,
      };
      next.lspDefinition = {
        tool: 'lspGetSemantics',
        query: { ...lspBase, type: 'definition' },
        why: 'Use the grep line as an LSP lineHint to resolve the symbol definition.',
        confidence: 'low',
      };
      next.lspReferences = {
        tool: 'lspGetSemantics',
        query: { ...lspBase, type: 'references' },
        why: 'Use the grep line as an LSP lineHint to inspect semantic usages.',
        confidence: 'low',
      };
    }
  }

  if (
    options.currentPage < options.totalFilePages &&
    options.currentPage < MAX_PAGE_NUMBER
  ) {
    next.nextPage = {
      tool: 'local.text',
      query: withoutUndefined({
        ...query,
        page: options.currentPage + 1,
      }),
      why: 'Continue to the next page of matched files.',
      confidence: 'exact',
    };
  }

  if (options.hasFileWithMoreMatches && options.matchPage < MAX_PAGE_NUMBER) {
    next.nextMatchPage = {
      tool: 'local.text',
      query: withoutUndefined({
        ...query,
        maxMatchesPerFile: options.matchesPerPage,
        matchPage: options.matchPage + 1,
      }),
      why: 'Continue within files that have more matches than this response returned.',
      confidence: 'exact',
    };
  }

  return next;
}

function lineRangeAroundMatch(match: FlowMatch): {
  startLine: number;
  endLine: number;
} {
  const line = Math.max(1, match.line ?? 1);
  const endLine = Math.max(line, match.endLine ?? line);
  return {
    startLine: Math.max(1, line - FETCH_CONTEXT_LINES),
    endLine: endLine + FETCH_CONTEXT_LINES,
  };
}

// LSP next-call inference is intentionally conservative: a wrong symbolName
// sends the agent to resolve a bogus anchor. We only infer when the evidence is
// an exact bare identifier, never from regex syntax, literals, dotted property
// text, multi-token snippets, windowed context, or aggregate count output.
export type InferredLspSymbol = {
  symbol: string;
  /**
   * Precise capture line from `metavarRanges` (structural search only).
   * Undefined when the search engine doesn't carry per-capture ranges — the
   * caller falls back to the match's own start line.
   */
  line?: number;
};

export function inferLspSymbolName(
  match: FlowMatch | undefined,
  query: RipgrepQuery,
  searchEngine: LocalSearchEngine
): InferredLspSymbol | undefined {
  // Aggregate / count output has no single-symbol anchor.
  if (
    query.output === 'countLines' ||
    query.output === 'countMatches' ||
    query.unique === 'list' ||
    query.unique === 'count'
  ) {
    return undefined;
  }

  // Structural search may only infer from a metavar whose full capture is one
  // bare identifier (e.g. `$NAME` bound to `getUser`, never to `false`).
  if (searchEngine === 'structural') {
    return firstBareIdentifierMetavar(match?.metavars, match?.metavarRanges);
  }

  // Windowed matches carry surrounding context, not a clean token.
  if (query.matchWindow) return undefined;

  // onlyMatching returns the exact matched substring — infer when it is itself a
  // bare identifier.
  if (query.output === 'matchOnly') {
    const symbol = bareIdentifier(match?.value);
    return symbol ? { symbol } : undefined;
  }

  // Otherwise infer only from an exact bare-identifier query. This suppresses
  // regex-like queries (`\w+_searched`), dotted fixed strings (`query.symbolName`),
  // and multi-token snippets, none of which are a single bare identifier.
  const symbol = bareIdentifier(query.searchText);
  return symbol ? { symbol } : undefined;
}

function firstBareIdentifierMetavar(
  metavars: Record<string, string[]> | undefined,
  metavarRanges: Record<string, Array<{ line: number }>> | undefined
): InferredLspSymbol | undefined {
  if (!metavars) return undefined;
  // Native capture maps have no stable iteration order. An explicit NAME/name
  // capture is the strongest available intent; other keys use code-unit order.
  // Captures carry no syntax role, so this remains a low-confidence suggestion.
  const keys = Object.keys(metavars).sort((left, right) => {
    const namePriority =
      Number(right.toLowerCase() === 'name') -
      Number(left.toLowerCase() === 'name');
    return namePriority || (left < right ? -1 : left > right ? 1 : 0);
  });
  for (const key of keys) {
    const values = metavars[key] ?? [];
    for (const [index, value] of values.entries()) {
      const symbol = bareIdentifier(value);
      // metavarRanges is parallel to metavars (same keys, same order) — the
      // precise per-capture line lets the LSP lineHint point at the captured
      // symbol itself instead of the whole match's (possibly multi-line) start.
      if (symbol) return { symbol, line: metavarRanges?.[key]?.[index]?.line };
    }
  }
  return undefined;
}

function bareIdentifier(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  const trimmed = value.trim();
  if (!BARE_IDENTIFIER.test(trimmed)) return undefined;
  if (RESERVED_SYMBOL_WORDS.has(trimmed)) return undefined;
  return trimmed;
}

function withoutUndefined(
  value: Record<string, unknown>
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(value).filter(([, item]) => item !== undefined)
  );
}
