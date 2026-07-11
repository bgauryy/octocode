import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

import type { SearchStats } from '../../../utils/core/types.js';
import { RESOURCE_LIMITS } from '../../../utils/core/constants.js';
import type { RipgrepQuery } from '../scheme.js';
import {
  rankFiles,
  isLowSignalQueryPath,
  type FileScore,
  type RankContext,
  type RankSort,
  type RankingProfileId,
} from '../rankingProfile.js';

import { buildSearchNextMap, type SearchNextMap } from './searchNext.js';

export type LocalSearchEngine = 'rg' | 'structural';

type CountedLocalSearchFile = LocalSearchCodeFile & {
  totalOccurrences?: number;
  totalMatchedLines?: number;
  totalMatchRows?: number;
  returnedMatchRows?: number;
};

type LocalSearchResultWithNext = LocalSearchCodeToolResult & {
  next?: SearchNextMap;
};

export async function buildSearchResult(
  parsedFiles: LocalSearchCodeFile[],
  configuredQuery: RipgrepQuery,
  searchEngine: LocalSearchEngine,
  warnings: string[],
  stats?: SearchStats
): Promise<LocalSearchCodeToolResult> {
  // Structural (AST) matches are already precise — a `call_expression` match
  // IS a call, with no comment/string noise for a relevance scorer to filter.
  // Default them to deterministic source/path order (matching ast-grep), and
  // reserve the language-aware relevance scorer for noisy text search. An
  // explicit `sort` always wins for either engine.
  const defaultSort: RankSort =
    searchEngine === 'structural' ? 'path' : 'relevance';
  const sort: RankSort = (configuredQuery.sort as RankSort) ?? defaultSort;
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

type RankingDebug = {
  score: number;
  profile: RankingProfileId;
  pathRole: FileScore['pathRole'];
  reasons: string[];
};

/** Build the deterministic ranking context from the validated query. */
function buildRankContext(query: RipgrepQuery): RankContext {
  const profileOverride = query.rankingProfile as
    RankContext['profileOverride'] | undefined;
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
