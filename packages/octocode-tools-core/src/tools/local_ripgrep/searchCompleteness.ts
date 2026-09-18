import type { RipgrepQuery } from '@octocodeai/octocode-core/schema';
import type { LocalSearchCodeToolResult } from '@octocodeai/octocode-core/extra-types';
import type { SearchStats } from '../../utils/core/types.js';
import { createErrorResult } from '../../utils/file/toolHelpers.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';

/** Shared native coverage policy for empty and nonempty search responses. */
export function nativeSearchPartialReasons(
  stats?: Pick<SearchStats, 'capped' | 'errorCount'>
): Array<'nativeResultCap' | 'nativeSearchError'> {
  return [
    ...(stats?.capped ? ['nativeResultCap' as const] : []),
    ...((stats?.errorCount ?? 0) > 0 ? ['nativeSearchError' as const] : []),
  ];
}

export function staleSearchSnapshotResult(
  query: RipgrepQuery,
  reason: string
): LocalSearchCodeToolResult {
  const { snapshot: _snapshot, ...restartQuery } = query;
  return createErrorResult(
    new Error(
      `Search snapshot cannot be continued (${reason}); restart the search.`
    ),
    query,
    {
      toolName: TOOL_NAMES.LOCAL_RIPGREP,
      extra: {
        errorCode: 'staleSnapshot',
        snapshotReason: reason,
        next: {
          restart: {
            tool: TOOL_NAMES.LOCAL_RIPGREP,
            query: { ...restartQuery, page: 1, matchPage: 1 },
            why: 'Start a new search against the current source.',
            confidence: 'exact',
          },
        },
      },
    }
  ) as LocalSearchCodeToolResult;
}
