import { RipgrepCommandBuilder } from '../../commands/RipgrepCommandBuilder.js';
import { safeExec } from '../../utils/exec/safe.js';
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
import { validateRipgrepQuery } from '@octocodeai/octocode-core/schemas/runtime';
import { LocalRipgrepQuerySchema, type RipgrepQuery } from './scheme.js';
import { RESOURCE_LIMITS } from '../../utils/core/constants.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { getHints } from '../../hints/index.js';
import { parseRipgrepOutput } from './ripgrepParser.js';
import { buildSearchResult } from './ripgrepResultBuilder.js';
import { preflightValidateRipgrepPattern } from './patternValidation.js';
import { attachRawResponseChars } from '../../utils/response/charSavings.js';

export async function executeRipgrepSearchInternal(
  configuredQuery: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const validationWarnings: string[] = [];
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

  const builder = new RipgrepCommandBuilder();
  const { command, args } = builder.fromQuery(queryForExec).build();

  const result = await safeExec(command, args);

  if (result.code === null) {
    const timeoutMs = RESOURCE_LIMITS.DEFAULT_EXEC_TIMEOUT_MS;
    return attachRawResponseChars(
      {
        status: 'error',
        error: `Search timed out after ${timeoutMs / 1000} seconds.`,
        errorCode: LOCAL_TOOL_ERROR_CODES.COMMAND_TIMEOUT,
        searchEngine: 'rg',
        hints: [
          `Search timed out after ${timeoutMs / 1000} seconds.`,
          'Try a more specific path or add langType/include filters to narrow the search.',
          'Use filesOnly=true for faster discovery.',
          'Consider excluding large directories with excludeDir.',
          ...chunkingWarnings,
        ],
      } as LocalSearchCodeToolResult,
      result.stdout.length + result.stderr.length
    );
  }

  if (result.code === 1 || (result.success && !result.stdout.trim())) {
    return attachRawResponseChars(
      {
        status: 'empty',
        searchEngine: 'rg',
        warnings: [...validationWarnings, ...chunkingWarnings],
        hints: getHints(TOOL_NAMES.LOCAL_RIPGREP, 'empty', {
          keywords: query.keywords,
          path: query.path,
          langType: query.langType,
          include: query.include,
          excludeDir: query.excludeDir,
          fixedString: query.fixedString,
          caseSensitive: query.caseSensitive,
          mode: query.mode,
        } as Record<string, unknown>),
      } as LocalSearchCodeToolResult,
      result.stdout.length
    );
  }

  if (!result.success) {
    const isMissingPath =
      result.code === 2 && /No such file or directory/.test(result.stderr);
    const message = isMissingPath
      ? `Search path not found: ${query.path}. Verify it with localViewStructure or localFindFiles.`
      : `Ripgrep failed (exit code ${result.code}): ${result.stderr}`;
    return createErrorResult(new Error(message), query, {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      rawResponse: result.stdout.length + result.stderr.length,
    }) as LocalSearchCodeToolResult;
  }

  const parsed = parseRipgrepOutput(result.stdout, query);

  if (
    !queryForExec.filesOnly &&
    result.stdout.length > RESOURCE_LIMITS.LARGE_RESULT_BYTES_HINT
  ) {
    chunkingWarnings.push(
      `Result payload is large (~${Math.round(result.stdout.length / 1024)}KB).`
    );
  }

  const searchResult = await buildSearchResult(
    parsed.files,
    query,
    'rg',
    [...validationWarnings, ...chunkingWarnings],
    parsed.stats
  );
  return attachRawResponseChars(searchResult, result.stdout.length);
}
