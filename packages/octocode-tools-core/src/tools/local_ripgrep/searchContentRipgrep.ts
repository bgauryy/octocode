import type { RipgrepQuery as LocalRipgrepQuery } from './scheme.js';

type RipgrepQuery = LocalRipgrepQuery;
import { createErrorResult } from '../../utils/file/toolHelpers.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { executeRipgrepSearchInternal } from './ripgrepExecutor.js';
import { searchContentStructural } from './structuralSearch.js';

function applyWorkflowMode(query: RipgrepQuery): RipgrepQuery {
  const mode = query.mode;
  if (!mode) return query;

  const next: RipgrepQuery = { ...query };
  if (mode === 'discovery' && next.output === 'content') {
    next.output = 'files';
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
    // Ripgrep runs in-process inside the native engine, which is always present
    // (it is the core dependency) — so there is no binary-availability check and
    // no grep fallback any more.
    return await executeRipgrepSearchInternal(configuredQuery);
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);

    if (errorMessage.includes('Output size limit exceeded')) {
      return {
        status: 'error',
        error: errorMessage,
        errorCode: LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE,
      } as LocalSearchCodeToolResult;
    }

    return createErrorResult(error, configuredQuery, {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
    }) as LocalSearchCodeToolResult;
  }
}
