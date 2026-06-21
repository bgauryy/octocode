import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { validateRipgrepQuery } from '@octocodeai/octocode-core/schemas/runtime';
import { LocalRipgrepQuerySchema, type RipgrepQuery } from './scheme.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { buildSearchResult } from './ripgrepResultBuilder.js';
import { preflightValidateRipgrepPattern } from './patternValidation.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import {
  contextUtils,
  type RipgrepSearchOptions,
} from '../../utils/contextUtils.js';

/** Filesystem sorts the native engine understands. */
type EngineSort = 'path' | 'created' | 'modified' | 'accessed';

/**
 * TS-level relevance modes (relevance/matchCount) are not filesystem sorts;
 * map them to a stable `path` walk so the engine returns a deterministic input
 * order for the ranker. Real filesystem sorts pass through unchanged.
 */
function toEngineSort(sort: RipgrepQuery['sort']): EngineSort {
  if (sort === 'created' || sort === 'modified' || sort === 'accessed') {
    return sort;
  }
  return 'path';
}

/** Map the validated tool query onto the native engine's search options. */
function toSearchOptions(
  query: RipgrepQuery & { path: string }
): RipgrepSearchOptions {
  return {
    path: query.path,
    // keywords is required for every non-structural mode (schema-enforced);
    // structural search never reaches this executor.
    pattern: query.keywords ?? '',
    fixedString: query.fixedString,
    perlRegex: query.perlRegex,
    caseSensitive: query.caseSensitive,
    caseInsensitive: query.caseInsensitive,
    wholeWord: query.wholeWord,
    invertMatch: query.invertMatch,
    multiline: query.multiline,
    multilineDotall: query.multilineDotall,
    filesOnly: query.filesOnly,
    filesWithoutMatch: query.filesWithoutMatch,
    countLinesPerFile: query.countLinesPerFile,
    countMatchesPerFile: query.countMatchesPerFile,
    contextLines: query.contextLines,
    langType: query.langType,
    include: query.include,
    exclude: query.exclude,
    excludeDir: query.excludeDir,
    noIgnore: query.noIgnore,
    hidden: query.hidden,
    // The engine only understands filesystem sorts. TS-level relevance modes
    // (relevance/matchCount) are applied after the walk in ripgrepResultBuilder;
    // give the engine a stable deterministic walk so ranking inputs are stable.
    sort: toEngineSort(query.sort),
    sortReverse: query.sortReverse,
    // AST classification feeds language-aware ranking; only worth its parse
    // cost when relevance ordering is actually requested.
    classifyMatches: query.sort === 'relevance' || query.sort === undefined,
    maxSnippetChars: query.matchContentLength,
    onlyMatching: query.onlyMatching,
    unique: query.unique,
    countUnique: query.countUnique,
    matchWindow: query.matchWindow,
  };
}

/** Rough char size of the search payload, used for the raw-response metric. */
function estimateResponseChars(files: LocalSearchCodeFile[]): number {
  let total = 0;
  for (const file of files) {
    total += file.path.length;
    if (file.matches) {
      for (const match of file.matches) {
        total += match.value?.length ?? 0;
      }
    }
  }
  return total;
}

export async function executeRipgrepSearchInternal(
  configuredQuery: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const validationWarnings: string[] = [];
  // Keep this validation even when callers already parsed the query: this
  // internal executor is exported and tested directly, so it is its own trust
  // boundary for command/path construction.
  const runtimeValidation = validateRipgrepQuery(configuredQuery);
  if (!runtimeValidation.isValid) {
    return createErrorResult(
      new Error(
        `Query validation failed: ${runtimeValidation.errors.join('; ')}`
      ),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: { warnings: runtimeValidation.warnings },
      }
    ) as LocalSearchCodeToolResult;
  }
  validationWarnings.push(...runtimeValidation.warnings);

  const validation = LocalRipgrepQuerySchema.safeParse(configuredQuery);
  if (!validation.success) {
    const errors = validation.error.issues.map(issue => issue.message);
    return createErrorResult(
      new Error(`Query validation failed: ${errors.join(', ')}`),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: { warnings: validationWarnings },
      }
    ) as LocalSearchCodeToolResult;
  }
  const query = validation.data;

  if (!query.path) {
    return createErrorResult(new Error('Path is required for search'), query, {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      extra: { warnings: validationWarnings },
    }) as LocalSearchCodeToolResult;
  }
  const queryWithPath = query as RipgrepQuery & { path: string };
  const pathValidation = validateToolPath(
    queryWithPath,
    TOOL_NAMES.LOCAL_RIPGREP
  );
  if (!pathValidation.isValid) {
    return pathValidation.errorResult as LocalSearchCodeToolResult;
  }

  const queryForExec = {
    ...query,
    path: pathValidation.sanitizedPath,
  };

  const patternCheck = preflightValidateRipgrepPattern({
    // keywords is required for every non-structural mode (schema-enforced);
    // structural never reaches this executor.
    pattern: queryForExec.keywords ?? '',
    fixedString: queryForExec.fixedString,
    perlRegex: queryForExec.perlRegex,
  });
  if (!patternCheck.isValid) {
    return createErrorResult(
      new Error(`Pattern validation failed: ${patternCheck.errors.join('; ')}`),
      query,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: {
          warnings: [...validationWarnings, ...patternCheck.warnings],
        },
      }
    ) as LocalSearchCodeToolResult;
  }

  const chunkingWarnings: string[] = [...patternCheck.warnings];

  // Native, in-process ripgrep: no `rg` binary, no spawn. The walk runs on the
  // libuv thread pool, returning the same `{ files, stats }` shape the old
  // `rg --json` + parser path produced.
  let parsed;
  try {
    parsed = await contextUtils.searchRipgrep(toSearchOptions(queryForExec));
  } catch (error) {
    return createErrorResult(
      error instanceof Error ? error : new Error(String(error)),
      query,
      { toolName: TOOL_NAMES.LOCAL_RIPGREP }
    ) as LocalSearchCodeToolResult;
  }

  const files: LocalSearchCodeFile[] = parsed.files.map(f => ({
    path: f.path,
    matchCount: f.matchCount,
    matches: f.matches.map(m => {
      const match = {
        line: m.line,
        column: m.column,
        value: m.value,
      } as NonNullable<LocalSearchCodeFile['matches']>[number] & {
        count?: number;
        kind?: string;
        scoreHint?: number;
      };
      if (m.count !== undefined) match.count = m.count;
      // AST classification from the engine (Tier 1 Phase 2), when present.
      if (m.kind !== undefined) match.kind = m.kind;
      if (m.scoreHint !== undefined) match.scoreHint = m.scoreHint;
      return match;
    }),
  }));

  const responseChars = estimateResponseChars(files);
  const stats = {
    totalOccurrences: parsed.stats.matchCount,
    matchedLines: parsed.stats.matchedLines,
    filesMatched: parsed.stats.filesMatched,
    filesSearched: parsed.stats.filesSearched,
    bytesSearched: parsed.stats.bytesSearched ?? undefined,
    searchTime: parsed.stats.searchTime,
  };

  if (files.length === 0) {
    return attachRawResponseChars(
      {
        status: 'empty',
        searchEngine: 'rg',
        stats,
        warnings: [...validationWarnings, ...chunkingWarnings],
      } as LocalSearchCodeToolResult,
      responseChars
    );
  }

  if (
    !queryForExec.filesOnly &&
    responseChars > RESOURCE_LIMITS.LARGE_RESULT_BYTES_HINT
  ) {
    chunkingWarnings.push(
      `Result payload is large (~${Math.round(responseChars / 1024)}KB).`
    );
  }

  const searchResult = await buildSearchResult(
    files,
    query,
    'rg',
    [...validationWarnings, ...chunkingWarnings],
    stats
  );
  return attachRawResponseChars(searchResult, responseChars);
}
