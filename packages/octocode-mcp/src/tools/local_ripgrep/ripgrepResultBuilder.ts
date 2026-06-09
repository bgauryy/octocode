import type { z } from 'zod';
import type { RipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

type UpstreamRipgrepQuery = z.infer<typeof RipgrepQuerySchema>;
import type { SearchStats } from '../../utils/core/types.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { compareIsoDateDescending } from '../../utils/core/compare.js';
import { promises as fs } from 'fs';
type RipgrepQuery = UpstreamRipgrepQuery;

export async function buildSearchResult(
  parsedFiles: LocalSearchCodeFile[],
  configuredQuery: RipgrepQuery,
  _searchEngine: 'rg' | 'grep',
  warnings: string[],
  stats?: SearchStats
): Promise<LocalSearchCodeToolResult> {
  const filesWithCharOffsets = parsedFiles;

  const filesWithMetadata = await Promise.all(
    filesWithCharOffsets.map(async f => {
      const file: typeof f & { modified?: string } = { ...f };
      if (configuredQuery.showFileLastModified) {
        file.modified = await getFileModifiedTime(f.path);
      }
      return file;
    })
  );

  filesWithMetadata.sort((a, b) =>
    compareRipgrepFilesByRelevance(a, b, configuredQuery)
  );

  let limitedFiles = filesWithMetadata;
  let wasLimited = false;
  if (
    configuredQuery.maxFiles &&
    filesWithMetadata.length > configuredQuery.maxFiles
  ) {
    limitedFiles = filesWithMetadata.slice(0, configuredQuery.maxFiles);
    wasLimited = true;
  }

  const totalFiles = limitedFiles.length;
  const isFileListMode =
    configuredQuery.filesOnly ||
    configuredQuery.count ||
    configuredQuery.countMatches;
  const summedMatches = limitedFiles.reduce(
    (sum: number, f: LocalSearchCodeFile & { modified?: string }) =>
      sum + f.matchCount,
    0
  );
  const totalMatches = isFileListMode
    ? (stats?.matchCount ?? summedMatches)
    : summedMatches;

  const aligned = configuredQuery as {
    itemsPerPage?: number;
    matchesPerFile?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    page?: number;
  };
  const filesPerPage =
    aligned.itemsPerPage || RESOURCE_LIMITS.DEFAULT_FILES_PER_PAGE;
  const filePageNumber = aligned.page || 1;
  const totalFilePages = Math.ceil(totalFiles / filesPerPage);
  const startIdx = (filePageNumber - 1) * filesPerPage;
  const endIdx = Math.min(startIdx + filesPerPage, totalFiles);
  const paginatedFiles = limitedFiles.slice(startIdx, endIdx);

  const matchesPerPage =
    aligned.maxMatchesPerFile ||
    aligned.matchesPerFile ||
    RESOURCE_LIMITS.DEFAULT_MATCHES_PER_PAGE;

  const finalFiles: LocalSearchCodeFile[] = paginatedFiles.map(
    (file: LocalSearchCodeFile & { modified?: string }) => {
      const totalFileMatches = file.matches.length;
      const totalMatchPages = Math.ceil(totalFileMatches / matchesPerPage);
      const matchPage = Math.max(1, aligned.matchPage || 1);
      const matchStartIdx = (matchPage - 1) * matchesPerPage;
      const matchEndIdx = Math.min(
        matchStartIdx + matchesPerPage,
        totalFileMatches
      );
      const paginatedMatches = isFileListMode
        ? []
        : file.matches.slice(matchStartIdx, matchEndIdx);

      const result: LocalSearchCodeFile = {
        path: file.path,
        matchCount: isFileListMode ? file.matchCount || 1 : totalFileMatches,
        matches: paginatedMatches,
        pagination:
          !isFileListMode && totalFileMatches > matchesPerPage
            ? {
                currentPage: matchPage,
                totalPages: totalMatchPages,
                matchesPerPage,
                totalMatches: totalFileMatches,
                hasMore: matchPage < totalMatchPages,
              }
            : undefined,
      };
      if (configuredQuery.showFileLastModified && file.modified) {
        result.modified = file.modified;
      }
      return result;
    }
  );

  const paginationHints: string[] =
    filePageNumber < totalFilePages
      ? [
          `Page ${filePageNumber}/${totalFilePages} (${finalFiles.length} of ${totalFiles} files, ${totalMatches} matches). Next: page=${filePageNumber + 1}`,
        ]
      : totalFilePages > 0 && filePageNumber > totalFilePages
        ? [
            `Page ${filePageNumber} is outside range (1–${totalFilePages}). Use page=${totalFilePages}.`,
          ]
        : [];

  if (wasLimited) {
    paginationHints.push(
      `Results limited to ${configuredQuery.maxFiles} files (found ${filesWithMetadata.length} matching)`
    );
  }

  const filesWithMoreMatches = finalFiles.filter(f => f.pagination?.hasMore);
  if (filesWithMoreMatches.length > 0) {
    paginationHints.push(
      `Note: ${filesWithMoreMatches.length} file(s) have more matches — use matchPage=${(aligned.matchPage || 1) + 1} with maxMatchesPerFile to continue matches inside those files`
    );
  }

  const refinementHints = _getStructuredResultSizeHints(
    finalFiles,
    configuredQuery,
    totalMatches
  );

  const q = configuredQuery as Record<string, unknown>;
  const activeFilters: string[] = [];
  const includeGlobs = q.include as string[] | undefined;
  if (Array.isArray(includeGlobs) && includeGlobs.length > 0) {
    activeFilters.push(`include: ${includeGlobs.join(', ')}`);
  }
  const excludeGlobs = q.exclude as string[] | undefined;
  if (Array.isArray(excludeGlobs) && excludeGlobs.length > 0) {
    activeFilters.push(`exclude: ${excludeGlobs.join(', ')}`);
  }
  const excludeDir = q.excludeDir as string[] | undefined;
  if (Array.isArray(excludeDir) && excludeDir.length > 0) {
    activeFilters.push(`excludeDir: ${excludeDir.join(', ')}`);
  }
  const fileType = (q.type ?? q.langType) as string | undefined;
  if (fileType) activeFilters.push(`type: ${fileType}`);
  if (q.caseSensitive) activeFilters.push('case-sensitive');
  if (q.wholeWord) activeFilters.push('whole-word');
  if (activeFilters.length > 0) {
    refinementHints.unshift(`Active filters — ${activeFilters.join(' | ')}`);
  }

  const fullResult: LocalSearchCodeToolResult = {
    files: finalFiles,
    pagination: {
      currentPage: filePageNumber,
      totalPages: totalFilePages,
      filesPerPage,
      totalFiles,
      totalMatches,
      hasMore: filePageNumber < totalFilePages,
      ...(wasLimited ? { totalFilesFound: filesWithMetadata.length } : {}),
    },
    ...(warnings.length > 0 ? { warnings } : {}),
    hints: [
      ...(totalFiles > 0 && !isFileListMode
        ? [
            'Pass line numbers as lineHint to lspGetSemanticContent for definitions, references, or call flow.',
          ]
        : []),
      ...paginationHints,
      ...refinementHints,
    ],
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

function _getStructuredResultSizeHints(
  files: LocalSearchCodeFile[],
  query: RipgrepQuery,
  totalMatches: number
): string[] {
  const hints: string[] = [];

  if (totalMatches > 100 || files.length > 20) {
    const recoveries: string[] = [];
    if (!query.type && !query.include) recoveries.push('add type or include');
    if (!query.excludeDir?.length) recoveries.push('add excludeDir');
    if (query.pattern.length < 5) recoveries.push('lengthen pattern');
    if (recoveries.length > 0) {
      hints.push(
        `Large result set (${totalMatches} matches in ${files.length} files). Narrow: ${recoveries.join(', ')}.`
      );
    }
  }

  return hints;
}

async function getFileModifiedTime(
  filePath: string
): Promise<string | undefined> {
  try {
    const stats = await fs.stat(filePath);
    return stats.mtime.toISOString();
  } catch {
    return undefined;
  }
}

function compareRipgrepFilesByRelevance(
  a: LocalSearchCodeFile & { modified?: string },
  b: LocalSearchCodeFile & { modified?: string },
  query: RipgrepQuery
): number {
  const matchDelta = b.matchCount - a.matchCount;
  if (matchDelta !== 0) return matchDelta;

  if (query.showFileLastModified) {
    const modifiedDelta = compareIsoDateDescending(a.modified, b.modified);
    if (modifiedDelta !== 0) return modifiedDelta;
  }

  return a.path.localeCompare(b.path);
}
