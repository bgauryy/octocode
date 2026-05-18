/**
 * Main entry point for ripgrep search.
 *
 * Bundled `@vscode/ripgrep` is the only engine. The historical grep
 * fallback was removed in May-2026 cleanup — see `cleanup_contract.test.ts`.
 */
import {
  checkCommandAvailability,
  getMissingCommandError,
} from '../../utils/exec/commandAvailability.js';
import {
  applyWorkflowMode,
  type RipgrepQuery as UpstreamRipgrepQuery,
} from '@octocodeai/octocode-core';
import type { WithOptionalMeta } from '../../types/execution.js';

type RipgrepQuery = WithOptionalMeta<UpstreamRipgrepQuery>;
import { createErrorResult } from '../../utils/file/toolHelpers.js';
import { LOCAL_TOOL_ERROR_CODES } from '../../errors/localToolErrors.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core';
import { ToolErrors } from '../../errors/errorFactories.js';
import { executeRipgrepSearchInternal } from './ripgrepExecutor.js';

export async function searchContentRipgrep(
  query: RipgrepQuery
): Promise<LocalSearchCodeToolResult> {
  const configuredQuery = applyWorkflowMode(query as UpstreamRipgrepQuery);

  try {
    const rgAvailability = await checkCommandAvailability('rg');

    if (!rgAvailability.available) {
      const toolError = ToolErrors.commandNotAvailable(
        'rg',
        getMissingCommandError('rg')
      );
      return createErrorResult(toolError, configuredQuery, {
        toolName: TOOL_NAMES.LOCAL_RIPGREP,
      }) as LocalSearchCodeToolResult;
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
          'Searching everything? Add type filters or path restrictions to focus scope',
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
