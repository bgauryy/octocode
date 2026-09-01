import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

import type { SearchStats } from '../../../utils/core/types.js';
import { RESOURCE_LIMITS } from '../../../utils/core/constants.js';
import { MAX_MATCH_CONTENT_LENGTH, MAX_PAGE_NUMBER } from '../../../config.js';
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
  terminalLimit?: boolean;
  truncated?: boolean;
  partialReasons?: Array<'maxFiles'>;
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
  // They always sort deterministically (path order, matching ast-grep) unless
  // the caller picked a concrete non-relevance sort (matchCount/modified/...).
  // The schema defaults `sort` to "relevance" before this code runs, so a
  // `?? fallback` can never fire — the structural override must beat the
  // schema default explicitly, not rely on undefined.
  const requestedSort = (configuredQuery.sort as RankSort) ?? 'relevance';
  const sort: RankSort =
    searchEngine === 'structural' && requestedSort === 'relevance'
      ? 'path'
      : requestedSort;
  // Ranking enriches ordering; it must never gate results. Any unexpected
  // failure degrades to the engine's original order so every matched file is
  // still returned to the tool.
  let ranked: ReturnType<typeof rankFiles>;
  try {
    ranked = rankFiles(parsedFiles, sort, buildRankContext(configuredQuery), {
      debug: false,
      sortReverse: configuredQuery.sortReverse,
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
  const isPathListMode =
    configuredQuery.output === 'files' ||
    configuredQuery.output === 'filesWithout';
  const isCountMode =
    configuredQuery.output === 'countLines' ||
    configuredQuery.output === 'countMatches';
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
  // A `page` beyond `totalFilePages` makes `startIdx` exceed `totalFiles`, so
  // `.slice()` silently returns [] — the same footgun as the per-file
  // `matchPage` case below, just one level up (files, not matches-per-file).
  const isPageOutOfRange = totalFiles > 0 && startIdx >= totalFiles;
  if (isPageOutOfRange) {
    warnings.push(
      `page:${currentPage} is out of range (only ${totalFilePages} page(s), ${totalFiles} total file(s)) — returned 0 files. Use page:1..${totalFilePages}.`
    );
  }

  const matchesPerPage =
    aligned.maxMatchesPerFile || RESOURCE_LIMITS.DEFAULT_MATCHES_PER_PAGE;

  const finalFiles: CountedLocalSearchFile[] = paginatedFiles.map(
    (file: LocalSearchCodeFile) => {
      const totalFileMatches = file.matches?.length ?? 0;
      const totalMatchPages = Math.max(
        1,
        Math.ceil(totalFileMatches / matchesPerPage)
      );
      const matchPage = Math.max(1, aligned.matchPage || 1);
      const matchStartIdx = (matchPage - 1) * matchesPerPage;
      // `matchPage` is a page NUMBER; if the caller changes `maxMatchesPerFile`
      // between calls (a different page SIZE), the same page number can point
      // past the end of this file's matches under the new size — e.g. page 2
      // at 25/page when all 22 matches fit on page 1. `.slice()` would then
      // silently return [] with nothing distinguishing "no more matches" from
      // "you asked for a page that never existed under this cap". Surface it
      // explicitly instead.
      const isOutOfRange =
        totalFileMatches > 0 && matchStartIdx >= totalFileMatches;
      const matchEndIdx = Math.min(
        matchStartIdx + matchesPerPage,
        totalFileMatches
      );
      const paginatedMatches = isFileListMode
        ? undefined
        : file.matches?.slice(matchStartIdx, matchEndIdx);
      const returnedMatchRows = paginatedMatches?.length;

      if (isOutOfRange) {
        warnings.push(
          `${file.path}: matchPage:${matchPage} is out of range under maxMatchesPerFile:${matchesPerPage} (only ${totalMatchPages} page(s), ${totalFileMatches} total match(es) for this file) — returned 0 rows for this file. Use matchPage:1..${totalMatchPages}, or drop matchPage to re-derive it from the current maxMatchesPerFile.`
        );
      }

      const debugScore = rankDebug?.get(file.path);
      const result = {
        path: file.path,
        ...(isPathListMode
          ? {}
          : configuredQuery.output === 'countLines'
            ? { totalMatchedLines: file.matchCount || 1 }
            : configuredQuery.output === 'countMatches'
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
          !isFileListMode && (totalFileMatches > matchesPerPage || isOutOfRange)
            ? {
                currentPage: matchPage,
                totalPages: totalMatchPages,
                matchesPerPage,
                totalMatches: totalFileMatches,
                hasMore: matchPage < totalMatchPages,
                ...(matchPage < totalMatchPages && matchPage < MAX_PAGE_NUMBER
                  ? { nextMatchPage: matchPage + 1 }
                  : {}),
                ...(isOutOfRange ? { outOfRange: true } : {}),
              }
            : undefined,
      } as LocalSearchCodeFile & { ranking?: RankingDebug };
      return result;
    }
  );

  const filesWithMoreMatches = finalFiles.filter(f => f.pagination?.hasMore);
  const terminalLimit =
    (currentPage < totalFilePages && currentPage >= MAX_PAGE_NUMBER) ||
    (filesWithMoreMatches.length > 0 &&
      (aligned.matchPage || 1) >= MAX_PAGE_NUMBER) ||
    (stats?.capReached === true &&
      (configuredQuery.maxFiles ?? RESOURCE_LIMITS.MAX_FILES_DEFAULT) >=
        MAX_MATCH_CONTENT_LENGTH);

  const next = buildSearchNextMap(finalFiles, configuredQuery, searchEngine, {
    isFileListMode,
    currentPage,
    totalFilePages,
    matchPage: aligned.matchPage || 1,
    matchesPerPage,
    hasFileWithMoreMatches: filesWithMoreMatches.length > 0,
  });
  const currentMaxFiles =
    configuredQuery.maxFiles ?? RESOURCE_LIMITS.MAX_FILES_DEFAULT;
  if (
    stats?.capReached === true &&
    currentMaxFiles < MAX_MATCH_CONTENT_LENGTH
  ) {
    next.expandScan = {
      tool: 'local.text',
      query: {
        ...configuredQuery,
        maxFiles: Math.min(
          MAX_MATCH_CONTENT_LENGTH,
          Math.max(currentMaxFiles + 1, currentMaxFiles * 2)
        ),
        page: 1,
      },
      why: 'Re-run with a larger file-scan bound because this search is partial.',
      confidence: 'exact',
    };
  }

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
      ...(currentPage < totalFilePages && currentPage < MAX_PAGE_NUMBER
        ? { nextPage: currentPage + 1 }
        : {}),
      ...(isPageOutOfRange ? { outOfRange: true } : {}),
    },
    ...(warnings.length > 0 ? { warnings } : {}),
    ...(Object.keys(next).length > 0 ? { next } : {}),
    ...(terminalLimit ? { terminalLimit: true } : {}),
    ...(stats?.capReached === true
      ? { truncated: true, partialReasons: ['maxFiles' as const] }
      : {}),
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
    keyword: query.searchText,
    langType: query.langType,
    caseSensitive: query.caseMode === 'sensitive',
    wholeWord: query.wholeWord,
    profileOverride,
    explicitLowSignal,
  };
}
