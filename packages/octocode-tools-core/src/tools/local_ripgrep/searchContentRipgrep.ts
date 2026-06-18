import { checkCommandAvailability } from '../../utils/exec/commandAvailability.js';
import type { RipgrepQuery as LocalRipgrepQuery } from './scheme.js';

type RipgrepQuery = LocalRipgrepQuery;
import { createErrorResult } from '../../utils/file/toolHelpers.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { executeRipgrepSearchInternal } from './ripgrepExecutor.js';
import { executeGrepFallbackSearch } from './grepFallbackExecutor.js';
import { searchContentStructural } from './structuralSearch.js';

function applyWorkflowMode(query: RipgrepQuery): RipgrepQuery {
  const mode = query.mode;
  if (!mode) return query;

  const next: RipgrepQuery = { ...query };
  if (mode === 'discovery' && next.filesOnly === undefined) {
    next.filesOnly = true;
  } else if (mode === 'detailed' && next.contextLines === undefined) {
    next.contextLines = 3;
  }
  return next;
}

export async function searchContentRipgrep(
  query: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const configuredQuery = applyWorkflowMode(query);

  // Structural (AST) search runs on the context-utils engine, not ripgrep —
  // branch before the rg-availability check and the rg-only defaults below.
  if (configuredQuery.mode === 'structural') {
    return await searchContentStructural(configuredQuery);
  }

  if (configuredQuery.contextLines === undefined) {
    configuredQuery.contextLines = 2;
  }

  try {
    const rgAvailability = await checkCommandAvailability('rg');

    if (!rgAvailability.available) {
      return await executeGrepFallbackSearch(
        configuredQuery,
        rgAvailability.error
      );
    }

    return await executeRipgrepSearchInternal(configuredQuery);
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);

    if (errorMessage.includes('Output size limit exceeded')) {
      return {
        status: 'error',
        error: errorMessage,
        errorCode: LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE,
        hints: [
          'Output exceeded 10MB - your pattern matched too broadly. Think about why results exploded:',
          'Is the pattern too generic? Make it specific to target what you actually need',
          'Searching everything? Add langType filters or path restrictions to focus scope',
          'For node_modules: Target specific packages rather than searching the entire directory',
          'Need file names only? FIND_FILES searches metadata without reading content',
          'Strategy: Start with filesOnly=true to see what matched, then narrow before reading content',
        ],
      } as LocalSearchCodeToolResult;
    }

    return createErrorResult(error, configuredQuery, {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
    }) as LocalSearchCodeToolResult;
  }
}
