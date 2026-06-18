import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';

import { safeExec } from '../../utils/exec/safe.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { getHints } from '../../hints/index.js';
import { buildSearchResult } from './ripgrepResultBuilder.js';
import { preflightValidateRipgrepPattern } from './patternValidation.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';
import { LocalRipgrepQuerySchema, type RipgrepQuery } from './scheme.js';

const GREP_FALLBACK_WARNING =
  'Using grep fallback because bundled ripgrep is unavailable; advanced ripgrep-only options may be ignored.';

export async function executeGrepFallbackSearch(
  configuredQuery: RipgrepQuery,
  unavailableReason?: string
): Promise<LocalSearchCodeToolResult> {
  const validationWarnings: string[] = [];
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

  const queryForExec: RipgrepQuery & { path: string } = {
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

  const warnings = buildGrepFallbackWarnings(
    validationWarnings,
    patternCheck.warnings,
    unavailableReason,
    query
  );

  const { args, outputMode } = buildGrepArgs(queryForExec);
  const result = await safeExec('grep', args);

  if (result.code === null) {
    const timeoutMs = RESOURCE_LIMITS.DEFAULT_EXEC_TIMEOUT_MS;
    return attachRawResponseChars(
      {
        status: 'error',
        error: `Search timed out after ${timeoutMs / 1000} seconds.`,
        errorCode: LOCAL_TOOL_ERROR_CODES.COMMAND_TIMEOUT,
        searchEngine: 'grep',
        hints: [
          `Search timed out after ${timeoutMs / 1000} seconds.`,
          'Try a more specific path or add include/exclude filters to narrow the search.',
          'Use filesOnly=true for faster discovery.',
          ...warnings,
        ],
      } as LocalSearchCodeToolResult,
      result.stdout.length + result.stderr.length
    );
  }

  if (result.code === 1 || (result.success && !result.stdout.trim())) {
    return attachRawResponseChars(
      {
        status: 'empty',
        searchEngine: 'grep',
        warnings,
        hints: [
          ...getHints(TOOL_NAMES.LOCAL_RIPGREP, 'empty', {
            keywords: query.keywords,
            path: query.path,
            langType: query.langType,
            include: query.include,
            excludeDir: query.excludeDir,
            fixedString: query.fixedString,
            caseSensitive: query.caseSensitive,
            mode: query.mode,
            searchEngine: 'grep',
          } as Record<string, unknown>),
          'Try with ripgrep for better results when bundled ripgrep is repaired.',
        ],
      } as LocalSearchCodeToolResult,
      result.stdout.length
    );
  }

  if (!result.success) {
    return createErrorResult(
      new Error(
        `grep fallback failed (exit code ${result.code}): ${result.stderr}`
      ),
      query,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        rawResponse: result.stdout.length + result.stderr.length,
      }
    ) as LocalSearchCodeToolResult;
  }

  const parsed = parseGrepOutput(result.stdout, outputMode);
  const searchResult = await buildSearchResult(
    parsed.files,
    query,
    'grep',
    warnings,
    { matchCount: parsed.matchCount }
  );
  return attachRawResponseChars(
    {
      ...searchResult,
      searchEngine: 'grep',
    } as LocalSearchCodeToolResult,
    result.stdout.length
  );
}

type GrepOutputMode = 'matches' | 'files' | 'count';

function buildGrepArgs(query: RipgrepQuery & { path: string }): {
  args: string[];
  outputMode: GrepOutputMode;
} {
  const args: string[] = ['-R', '-H'];

  if (query.fixedString) {
    args.push('-F');
  } else if (query.perlRegex) {
    args.push('-E');
  }

  if (query.caseInsensitive || shouldApplySmartCase(query)) {
    args.push('-i');
  }

  if (query.wholeWord) args.push('-w');
  if (query.invertMatch) args.push('-v');
  args.push('-I');

  const context = query.contextLines;
  if (context !== undefined && context > 0) {
    args.push('-C', String(context));
  }

  const outputMode: GrepOutputMode = query.filesOnly
    ? 'files'
    : query.countLinesPerFile || query.countMatchesPerFile
      ? 'count'
      : 'matches';

  if (outputMode === 'files') {
    args.push('-l');
  } else if (outputMode === 'count') {
    args.push('-c');
  } else {
    args.push('-n');
  }

  for (const include of query.include ?? []) {
    args.push(`--include=${include}`);
  }
  for (const exclude of query.exclude ?? []) {
    args.push(`--exclude=${exclude}`);
  }
  for (const dir of query.excludeDir ?? []) {
    args.push(`--exclude-dir=${dir}`);
  }
  if (query.langType) {
    args.push(`--include=*.${query.langType}`);
  }

  args.push('--', query.keywords ?? '', query.path);

  return { args, outputMode };
}

function shouldApplySmartCase(query: RipgrepQuery): boolean {
  return (
    query.caseSensitive !== true &&
    query.caseInsensitive !== true &&
    (query.keywords ?? '') === (query.keywords ?? '').toLowerCase()
  );
}

function buildGrepFallbackWarnings(
  validationWarnings: string[],
  patternWarnings: string[],
  unavailableReason: string | undefined,
  query: RipgrepQuery
): string[] {
  const warnings = [GREP_FALLBACK_WARNING];
  if (unavailableReason) {
    warnings.push(`Ripgrep unavailable: ${unavailableReason}`);
  }
  warnings.push(...validationWarnings, ...patternWarnings);

  const ignored: string[] = [];
  if (query.filesWithoutMatch) ignored.push('filesWithoutMatch');
  if (query.countMatchesPerFile)
    ignored.push('countMatchesPerFile uses grep -c line counts');
  if (query.hidden) ignored.push('hidden');
  if (query.noIgnore) ignored.push('noIgnore');
  if (query.multiline) ignored.push('multiline');
  if (query.multilineDotall) ignored.push('multilineDotall');
  if (query.sort || query.sortReverse) ignored.push('sort');

  if (ignored.length > 0) {
    warnings.push(
      `grep fallback ignored/degraded options: ${ignored.join(', ')}.`
    );
  }

  return warnings;
}

function parseGrepOutput(
  stdout: string,
  outputMode: GrepOutputMode
): { files: LocalSearchCodeFile[]; matchCount: number } {
  if (outputMode === 'files') {
    const files = uniqueNonEmptyLines(stdout).map(path => ({
      path,
      matchCount: 1,
      matches: [],
    }));
    return { files, matchCount: files.length };
  }

  if (outputMode === 'count') {
    const files = uniqueNonEmptyLines(stdout)
      .map(parseGrepCountLine)
      .filter((file): file is LocalSearchCodeFile => file !== undefined);
    const matchCount = files.reduce(
      (sum, file) => sum + (file.matchCount ?? 0),
      0
    );
    return { files, matchCount };
  }

  const byFile = new Map<string, LocalSearchCodeFile>();
  for (const line of stdout.split('\n')) {
    if (!line.trim() || line === '--') continue;
    const parsed = parseGrepMatchLine(line);
    if (!parsed) continue;

    const file = byFile.get(parsed.path) ?? {
      path: parsed.path,
      matchCount: 0,
      matches: [],
    };
    const fileMatches = (file.matches ??= []);
    fileMatches.push({
      value: parsed.value,
      line: parsed.line,
      column: 0,
    });
    file.matchCount = fileMatches.length;
    byFile.set(parsed.path, file);
  }

  const files = [...byFile.values()];
  const matchCount = files.reduce(
    (sum, file) => sum + (file.matchCount ?? 0),
    0
  );
  return { files, matchCount };
}

function uniqueNonEmptyLines(stdout: string): string[] {
  return [...new Set(stdout.trim().split('\n').filter(Boolean))];
}

function parseGrepCountLine(line: string): LocalSearchCodeFile | undefined {
  const split = splitPathNumberRest(line);
  if (!split) return undefined;
  const count = Number.parseInt(split.numberText, 10);
  if (!Number.isFinite(count) || count <= 0) return undefined;
  return { path: split.path, matchCount: count, matches: [] };
}

function parseGrepMatchLine(
  line: string
): { path: string; line: number; value: string } | undefined {
  const split = splitPathNumberRest(line);
  if (!split) return undefined;
  const lineNumber = Number.parseInt(split.numberText, 10);
  if (!Number.isFinite(lineNumber)) return undefined;
  return {
    path: split.path,
    line: lineNumber,
    value: split.rest,
  };
}

function splitPathNumberRest(
  line: string
): { path: string; numberText: string; rest: string } | undefined {
  const firstColon = line.indexOf(':');
  if (firstColon === -1) return undefined;
  const secondColon = line.indexOf(':', firstColon + 1);
  if (secondColon === -1) return undefined;
  return {
    path: line.slice(0, firstColon),
    numberText: line.slice(firstColon + 1, secondColon),
    rest: line.slice(secondColon + 1),
  };
}
