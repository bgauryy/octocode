import type { RipgrepQuery as LocalRipgrepQuery } from '@octocodeai/octocode-core/schema';

type RipgrepQuery = LocalRipgrepQuery;
import { createErrorResult } from '../../utils/file/toolHelpers.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import { applyWorkflowMode } from './queryWorkflow.js';
import { executeRipgrepSearchInternal } from './ripgrepExecutor.js';
import { searchContentStructural } from './structuralSearch.js';

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
