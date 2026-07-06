import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import {
  createResponseFormat,
  sanitizeStructuredContent,
} from '../../responses.js';
import type { BulkFinalizer, BulkToolResponse } from '../../types/bulk.js';
import type { FlatQueryResult } from '../../types/toolResults.js';
import {
  hoistSharedFields,
  relativizeResultPaths,
} from '../../utils/response/pathRelativize.js';
import { fetchContent } from './fetchContent.js';
import {
  LocalFetchContentQuerySchema,
  type FetchContentQuery,
} from './scheme.js';
import { safeParseOrError } from '../utils.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
export { finalizeFetchContentResult } from './fetchContent.js';

type LocalFetchContentResponse = BulkToolResponse & Record<string, unknown>;

export async function executeFetchContent(
  args: ToolExecutionArgs<FetchContentQuery>
): Promise<CallToolResult> {
  const { queries } = args;

  return executeBulkOperation(
    queries || [],
    async (query: FetchContentQuery) =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
        query,
        contextMessage: 'localGetFileContent execution failed',
        execute: async () => {
          const parsed = safeParseOrError<FetchContentQuery>(
            LocalFetchContentQuerySchema,
            query
          );
          if (parsed.ok === false) {
            return parsed.error;
          }
          const result = await fetchContent(parsed.data);
          return result;
        },
      }),
    {
      toolName: TOOL_NAMES.LOCAL_FETCH_CONTENT,
      finalize: buildLocalFetchContentFinalizer<FetchContentQuery>(),
    },
    args
  );
}

function buildLocalFetchContentFinalizer<
  TQuery extends FetchContentQuery,
>(): BulkFinalizer<TQuery, LocalFetchContentResponse> {
  return ({ results }) => {
    const responseData: BulkToolResponse = {
      results: cloneFlatResults(results),
    };

    const dataBase = relativizeResultPaths(responseData.results);
    if (dataBase) responseData.base = dataBase;

    const shared = hoistSharedFields(responseData.results);
    if (shared) responseData.shared = shared;

    return {
      structuredContent: sanitizeStructuredContent(
        responseData
      ) as LocalFetchContentResponse,
      text: formatLocalFetchContentText(responseData),
      isError:
        responseData.results.length > 0 &&
        responseData.results.every(
          queryResult => queryResult.status === 'error'
        ),
    };
  };
}

function cloneFlatResults(
  results: readonly FlatQueryResult[]
): FlatQueryResult[] {
  return results.map(result => ({
    ...result,
    data: structuredClone(result.data),
  }));
}

function formatLocalFetchContentText(responseData: BulkToolResponse): string {
  const lines: string[] = [];

  if (responseData.base) {
    lines.push(`base: ${responseData.base}`, '');
  }

  for (const result of responseData.results) {
    const data = result.data;
    const content = typeof data.content === 'string' ? data.content : undefined;
    const displayData = { ...data };
    delete displayData.content;

    lines.push(
      `result: ${result.id}${result.status ? ` (${result.status})` : ''}`
    );

    const metadata = createResponseFormat({ data: displayData }, [
      'data',
      'path',
      'resolvedPath',
      'contentView',
      'startLine',
      'endLine',
      'totalLines',
      'isPartial',
      'pagination',
      'sourceChars',
      'sourceBytes',
      'warnings',
      'error',
    ]).trimEnd();
    if (metadata) lines.push(metadata);

    if (content !== undefined) {
      lines.push('content (copy-safe):', content);
    }
    lines.push('');
  }

  if (responseData.shared) {
    lines.push(
      createResponseFormat({ shared: responseData.shared }, [
        'shared',
      ]).trimEnd()
    );
  }

  return lines.join('\n').trimEnd() + '\n';
}
