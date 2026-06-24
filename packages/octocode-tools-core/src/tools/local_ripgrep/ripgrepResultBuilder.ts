import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

import type { SearchStats } from '../../utils/core/types.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import type { RipgrepQuery } from './scheme.js';
import {
  rankFiles,
  isLowSignalQueryPath,
  type FileScore,
  type RankContext,
  type RankSort,
  type RankingProfileId,
} from './rankingProfile.js';

export type LocalSearchEngine = 'rg' | 'structural';

type CountedLocalSearchFile = LocalSearchCodeFile & {
  totalOccurrences?: number;
  totalMatchedLines?: number;
  totalMatchRows?: number;
  returnedMatchRows?: number;
};

type NextToolName =
  | 'localGetFileContent'
  | 'lspGetSemantics'
  | 'localSearchCode';

type NextConfidence = 'exact' | 'heuristic';

type SearchNextCall = {
  tool: NextToolName;
  query: Record<string, unknown>;
  why: string;
  confidence?: NextConfidence;
};

type SearchNextMap = {
  fetchExact?: SearchNextCall;
  fetchStandard?: SearchNextCall;
  fetchSymbols?: SearchNextCall;
  lspDefinition?: SearchNextCall;
  lspReferences?: SearchNextCall;
  nextPage?: SearchNextCall;
  nextMatchPage?: SearchNextCall;
};

type LocalSearchResultWithNext = LocalSearchCodeToolResult & {
  next?: SearchNextMap;
};

type FlowMatch = {
  line?: number;
  endLine?: number;
  value?: string;
  metavars?: Record<string, string[]>;
};

type FlowFile = {
  path?: string;
  matches?: FlowMatch[];
  pagination?: { hasMore?: boolean };
};

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

export async function buildSearchResult(
  parsedFiles: LocalSearchCodeFile[],
  configuredQuery: RipgrepQuery,
  searchEngine: LocalSearchEngine,
  warnings: string[],
  stats?: SearchStats
): Promise<LocalSearchCodeToolResult> {
  const sort: RankSort = (configuredQuery.sort as RankSort) ?? 'relevance';
  // Ranking enriches ordering; it must never gate results. Any unexpected
  // failure degrades to the engine's original order so every matched file is
  // still returned to the tool.
  let ranked: ReturnType<typeof rankFiles>;
  try {
    ranked = rankFiles(parsedFiles, sort, buildRankContext(configuredQuery), {
      debug: Boolean(configuredQuery.debugRanking),
    });
  } catch {
    ranked = { files: parsedFiles, cappedCandidates: 0 };
    warnings.push(
      'Relevance ranking failed; returning results in unranked engine order.'
    );
  }
  const filesWithMetadata = ranked.files;
  const rankDebug = ranked.debug;

  // `maxFiles` is a PER-PAGE size ceiling (cost bound), NOT a lossy hard cap.
  // We paginate over the FULL ranked set so every matched file stays reachable
  // by paging; `totalFiles` is the true total of the ranked set.
  const totalFiles = filesWithMetadata.length;
  const isPathListMode = Boolean(
    configuredQuery.filesOnly || configuredQuery.filesWithoutMatch
  );
  const isCountMode = Boolean(
    configuredQuery.countLinesPerFile || configuredQuery.countMatchesPerFile
  );
  const isFileListMode = isPathListMode || isCountMode;
  const summedMatches = filesWithMetadata.reduce(
    (sum: number, f: LocalSearchCodeFile & { modified?: string }) =>
      sum + (f.matchCount ?? 0),
    0
  );
  const totalMatches = isFileListMode
    ? (stats?.totalOccurrences ??
      stats?.totalStructuralMatches ??
      summedMatches)
    : summedMatches;

  const aligned = configuredQuery as {
    itemsPerPage?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    page?: number;
  };
  const filesPerPage = Math.min(
    aligned.itemsPerPage || RESOURCE_LIMITS.DEFAULT_FILES_PER_PAGE,
    configuredQuery.maxFiles || Number.POSITIVE_INFINITY
  );
  const currentPage = aligned.page || 1;
  const totalFilePages = Math.max(1, Math.ceil(totalFiles / filesPerPage));
  const startIdx = (currentPage - 1) * filesPerPage;
  const endIdx = Math.min(startIdx + filesPerPage, totalFiles);
  const paginatedFiles = filesWithMetadata.slice(startIdx, endIdx);

  const matchesPerPage =
    aligned.maxMatchesPerFile || RESOURCE_LIMITS.DEFAULT_MATCHES_PER_PAGE;

  const finalFiles: CountedLocalSearchFile[] = paginatedFiles.map(
    (file: LocalSearchCodeFile) => {
      const totalFileMatches = file.matches?.length ?? 0;
      const totalMatchPages = Math.ceil(totalFileMatches / matchesPerPage);
      const matchPage = Math.max(1, aligned.matchPage || 1);
      const matchStartIdx = (matchPage - 1) * matchesPerPage;
      const matchEndIdx = Math.min(
        matchStartIdx + matchesPerPage,
        totalFileMatches
      );
      const paginatedMatches = isFileListMode
        ? undefined
        : file.matches?.slice(matchStartIdx, matchEndIdx);
      const returnedMatchRows = paginatedMatches?.length;

      const debugScore = rankDebug?.get(file.path);
      const result = {
        path: file.path,
        ...(isPathListMode
          ? {}
          : configuredQuery.countLinesPerFile
            ? { totalMatchedLines: file.matchCount || 1 }
            : configuredQuery.countMatchesPerFile
              ? { totalOccurrences: file.matchCount || 1 }
              : {
                  totalMatchRows: totalFileMatches,
                  ...(returnedMatchRows !== undefined
                    ? { returnedMatchRows }
                    : {}),
                }),
        ...(paginatedMatches !== undefined && { matches: paginatedMatches }),
        ...(debugScore
          ? {
              ranking: {
                score: debugScore.score,
                profile: debugScore.profile,
                pathRole: debugScore.pathRole,
                reasons: debugScore.reasons,
              },
            }
          : {}),
        pagination:
          !isFileListMode && totalFileMatches > matchesPerPage
            ? {
                currentPage: matchPage,
                totalPages: totalMatchPages,
                matchesPerPage,
                totalMatches: totalFileMatches,
                hasMore: matchPage < totalMatchPages,
                ...(matchPage < totalMatchPages
                  ? { nextMatchPage: matchPage + 1 }
                  : {}),
              }
            : undefined,
      } as LocalSearchCodeFile & { ranking?: RankingDebug };
      return result;
    }
  );

  const filesWithMoreMatches = finalFiles.filter(f => f.pagination?.hasMore);

  const next = buildSearchNextMap(finalFiles, configuredQuery, searchEngine, {
    isFileListMode,
    currentPage,
    totalFilePages,
    matchPage: aligned.matchPage || 1,
    matchesPerPage,
    hasFileWithMoreMatches: filesWithMoreMatches.length > 0,
  });

  const fullResult: LocalSearchResultWithNext = {
    searchEngine,
    ...(stats ? { stats } : {}),
    files: finalFiles,
    pagination: {
      currentPage,
      totalPages: totalFilePages,
      filesPerPage,
      totalFiles,
      ...(isPathListMode ? {} : { totalMatches }),
      hasMore: currentPage < totalFilePages,
      ...(currentPage < totalFilePages ? { nextPage: currentPage + 1 } : {}),
    },
    ...(warnings.length > 0 ? { warnings } : {}),
    ...(Object.keys(next).length > 0 ? { next } : {}),
  };

  return finalizeRipgrepResult(fullResult, configuredQuery, {
    totalMatches,
    totalFiles,
  });
}

export function finalizeRipgrepResult(
  result: LocalSearchCodeToolResult,
  _query: RipgrepQuery,
  _totals: { totalMatches: number; totalFiles: number }
): LocalSearchCodeToolResult {
  return result;
}

function buildSearchNextMap(
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
  }
): SearchNextMap {
  const firstFile = (files as FlowFile[]).find(file => file.path);
  const firstMatch = firstFile?.matches?.find(match => match.line);
  const next: SearchNextMap = {};

  if (firstFile?.path) {
    if (firstMatch?.line) {
      const range = lineRangeAroundMatch(firstMatch);
      next.fetchExact = {
        tool: 'localGetFileContent',
        query: withoutUndefined({
          path: firstFile.path,
          startLine: range.startLine,
          endLine: range.endLine,
          minify: 'none',
        }),
        why: 'Read exact source around the first grep match before editing, quoting, or validating comments/tests.',
        confidence: 'exact',
      };
      next.fetchStandard = {
        tool: 'localGetFileContent',
        query: withoutUndefined({
          path: firstFile.path,
          startLine: range.startLine,
          endLine: range.endLine,
          minify: 'standard',
        }),
        why: 'Read a token-efficient source slice around the first grep match.',
        confidence: 'exact',
      };
    } else if (options.isFileListMode) {
      next.fetchStandard = {
        tool: 'localGetFileContent',
        query: { path: firstFile.path, minify: 'standard' },
        why: 'Read the first matched file from file-list/count mode.',
        confidence: 'heuristic',
      };
    }

    next.fetchSymbols = {
      tool: 'localGetFileContent',
      query: { path: firstFile.path, minify: 'symbols' },
      why: 'Get a symbol skeleton for fast orientation before opening large bodies.',
      confidence: 'exact',
    };

    const symbolName = inferLspSymbolName(firstMatch, query, searchEngine);
    if (symbolName && firstMatch?.line) {
      const lspBase = {
        uri: firstFile.path,
        symbolName,
        lineHint: firstMatch.line,
      };
      next.lspDefinition = {
        tool: 'lspGetSemantics',
        query: { ...lspBase, type: 'definition' },
        why: 'Use the grep line as an LSP lineHint to resolve the symbol definition.',
        confidence: 'heuristic',
      };
      next.lspReferences = {
        tool: 'lspGetSemantics',
        query: { ...lspBase, type: 'references' },
        why: 'Use the grep line as an LSP lineHint to inspect semantic usages.',
        confidence: 'heuristic',
      };
    }
  }

  if (options.currentPage < options.totalFilePages) {
    next.nextPage = {
      tool: 'localSearchCode',
      query: withoutUndefined({
        ...query,
        page: options.currentPage + 1,
      }),
      why: 'Continue to the next page of matched files.',
      confidence: 'exact',
    };
  }

  if (options.hasFileWithMoreMatches) {
    next.nextMatchPage = {
      tool: 'localSearchCode',
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
export function inferLspSymbolName(
  match: FlowMatch | undefined,
  query: RipgrepQuery,
  searchEngine: LocalSearchEngine
): string | undefined {
  // Aggregate / count output has no single-symbol anchor.
  if (
    query.countLinesPerFile ||
    query.countMatchesPerFile ||
    query.countUnique ||
    query.unique
  ) {
    return undefined;
  }

  // Structural search may only infer from a metavar whose full capture is one
  // bare identifier (e.g. `$NAME` bound to `getUser`, never to `false`).
  if (searchEngine === 'structural') {
    return firstBareIdentifierMetavar(match?.metavars);
  }

  // Windowed matches carry surrounding context, not a clean token.
  if (query.matchWindow) return undefined;

  // onlyMatching returns the exact matched substring — infer when it is itself a
  // bare identifier.
  if (query.onlyMatching) {
    return bareIdentifier(match?.value);
  }

  // Otherwise infer only from an exact bare-identifier query. This suppresses
  // regex-like queries (`\w+_searched`), dotted fixed strings (`query.symbolName`),
  // and multi-token snippets, none of which are a single bare identifier.
  return bareIdentifier(query.keywords);
}

function firstBareIdentifierMetavar(
  metavars: Record<string, string[]> | undefined
): string | undefined {
  if (!metavars) return undefined;
  for (const values of Object.values(metavars)) {
    for (const value of values) {
      const symbol = bareIdentifier(value);
      if (symbol) return symbol;
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

type RankingDebug = {
  score: number;
  profile: RankingProfileId;
  pathRole: FileScore['pathRole'];
  reasons: string[];
};

/** Build the deterministic ranking context from the validated query. */
function buildRankContext(query: RipgrepQuery): RankContext {
  const profileOverride = query.rankingProfile as
    | RankContext['profileOverride']
    | undefined;
  // If the user explicitly scoped the search into a low-signal/test/docs area
  // (via include globs or a path that targets such an area), don't penalize
  // those roles. Path detection is anchored to segments — "latest/" / "contest/"
  // must NOT count (Fix #1).
  const explicitLowSignal = Boolean(
    query.include?.length || isLowSignalQueryPath(query.path)
  );
  return {
    queryPath: query.path,
    keyword: query.keywords,
    langType: query.langType,
    caseSensitive: query.caseSensitive,
    wholeWord: query.wholeWord,
    profileOverride,
    explicitLowSignal,
  };
}
