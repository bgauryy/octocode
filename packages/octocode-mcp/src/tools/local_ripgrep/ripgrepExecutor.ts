import { RipgrepCommandBuilder } from '../../commands/RipgrepCommandBuilder.js';
import { safeExec } from '../../utils/exec/safe.js';
import type { z } from 'zod';
import { validateRipgrepQuery } from '@octocodeai/octocode-core/schemas/runtime';
import type { RipgrepQuerySchema } from '@octocodeai/octocode-core/schemas';

type RipgrepQuery = z.infer<typeof RipgrepQuerySchema>;
import {
  validateToolPath,
  createErrorResult,
} from '../../utils/file/toolHelpers.js';
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
  const validation = validateRipgrepQuery(configuredQuery);
  if (!validation.isValid) {
    return createErrorResult(
      new Error(`Query validation failed: ${validation.errors.join(', ')}`),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: { warnings: validation.warnings },
      }
    ) as LocalSearchCodeToolResult;
  }

  if (!configuredQuery.path) {
    return createErrorResult(
      new Error('Path is required for search'),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: { warnings: validation.warnings },
      }
    ) as LocalSearchCodeToolResult;
  }
  const queryWithPath = configuredQuery as RipgrepQuery & { path: string };
  const pathValidation = validateToolPath(
    queryWithPath,
    TOOL_NAMES.LOCAL_RIPGREP
  );
  if (!pathValidation.isValid) {
    return pathValidation.errorResult as LocalSearchCodeToolResult;
  }

  const queryForExec = {
    ...configuredQuery,
    path: pathValidation.sanitizedPath!,
  };

  const patternCheck = preflightValidateRipgrepPattern({
    pattern: queryForExec.pattern,
    fixedString: queryForExec.fixedString,
    perlRegex: queryForExec.perlRegex,
  });
  if (!patternCheck.isValid) {
    return createErrorResult(
      new Error(`Pattern validation failed: ${patternCheck.errors.join('; ')}`),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        extra: {
          warnings: [...validation.warnings, ...patternCheck.warnings],
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
          'Try a more specific path or add type/include filters to narrow the search.',
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
        warnings: [...validation.warnings, ...chunkingWarnings],
        hints: getHints(TOOL_NAMES.LOCAL_RIPGREP, 'empty', {
          pattern: configuredQuery.pattern,
          path: configuredQuery.path,
          type: configuredQuery.type,
          include: configuredQuery.include,
          excludeDir: configuredQuery.excludeDir,
          fixedString: configuredQuery.fixedString,
          caseSensitive: configuredQuery.caseSensitive,
        } as Record<string, unknown>),
      } as LocalSearchCodeToolResult,
      result.stdout.length
    );
  }

  if (!result.success) {
    return createErrorResult(
      new Error(`Ripgrep failed (exit code ${result.code}): ${result.stderr}`),
      configuredQuery,
      {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
        rawResponse: result.stdout.length + result.stderr.length,
      }
    ) as LocalSearchCodeToolResult;
  }

  const parsed = parseRipgrepOutput(result.stdout, configuredQuery);

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
    configuredQuery,
    'rg',
    [...validation.warnings, ...chunkingWarnings],
    parsed.stats
  );
  return attachRawResponseChars(searchResult, result.stdout.length);
}
