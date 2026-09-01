import type { CallToolResult } from '@modelcontextprotocol/server';

import { buildFileGraph, type WalkResult } from '../../graph/buildFileGraph.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  createErrorResult,
  validateToolPath,
} from '../../utils/file/toolHelpers.js';
import { executeBulkOperation } from '../../utils/response/bulk.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { TOOL_NAMES } from '../toolMetadata/proxies.js';
import { safeParseOrError } from '../utils.js';
import { analyzeGraph } from './analyzeGraph.js';
import {
  type AnalyzeGraphOutput,
  type AnalyzeGraphQuery,
  LocalAnalyzeGraphQuerySchema,
} from './scheme.js';

export async function executeAnalyzeGraph(
  args: ToolExecutionArgs<AnalyzeGraphQuery>
): Promise<CallToolResult> {
  const graphCache = new Map<string, Promise<WalkResult>>();
  const getGraph = (
    path: string,
    excludeDir: string[],
    maxFiles: number
  ): Promise<WalkResult> => {
    const key = JSON.stringify([path, [...excludeDir].sort(), maxFiles]);
    const existing = graphCache.get(key);
    if (existing) return existing;
    const pending = buildFileGraph(path, excludeDir, maxFiles);
    graphCache.set(key, pending);
    return pending;
  };

  return executeBulkOperation(
    args.queries || [],
    async query =>
      executeWithToolBoundary({
        toolName: TOOL_NAMES.LOCAL_ANALYZE_GRAPH,
        query,
        contextMessage: 'localAnalyzeGraph execution failed',
        execute: async () => {
          const parsed = safeParseOrError<AnalyzeGraphQuery>(
            LocalAnalyzeGraphQuerySchema,
            query
          );
          if (parsed.ok === false) return parsed.error;

          const pathValidation = validateToolPath(
            parsed.data,
            TOOL_NAMES.LOCAL_ANALYZE_GRAPH
          );
          if (!pathValidation.isValid) {
            return createErrorResult(pathValidation.errorResult, parsed.data, {
              toolName: TOOL_NAMES.LOCAL_ANALYZE_GRAPH,
            }) as AnalyzeGraphOutput;
          }

          return analyzeGraph(
            {
              ...parsed.data,
              path: pathValidation.sanitizedPath,
            },
            { getGraph }
          );
        },
      }),
    { toolName: TOOL_NAMES.LOCAL_ANALYZE_GRAPH },
    args
  );
}
