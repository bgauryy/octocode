import { isAbsolute } from 'node:path';

import type { CallToolResult } from '@modelcontextprotocol/server';

import { buildFileGraph, type WalkResult } from '../../graph/buildFileGraph.js';
import type { ToolExecutionArgs } from '../../types/execution.js';
import {
  createErrorResult,
  validateToolPath,
} from '../../utils/file/toolHelpers.js';
import { executeBulkOperation } from '../../utils/response/bulk/response.js';
import { executeWithToolBoundary } from '../executionGuard.js';
import { TOOL_NAMES } from '../toolMetadata/names.js';
import { safeParseOrError } from '../utils.js';
import { analyzeGraph, inferRootFromAbsoluteFile } from './analyzeGraph.js';
import {
  type AnalyzeGraphOutput,
  type AnalyzeGraphQuery,
  LocalAnalyzeGraphQuerySchema,
} from './scheme.js';

/**
 * When `path` is omitted, try to derive the repository root from the first
 * absolute file-like field in the query.  The walk stops at the nearest
 * `package.json`, matching the behaviour of the impl-level fallback in
 * `analyzeGraph()` so both call paths are consistent.
 */
function inferPathIfMissing(query: AnalyzeGraphQuery): AnalyzeGraphQuery {
  if (query.path) return query;
  // `file`, `target`, and `entrypoints` are only present on specific
  // discriminated-union variants; cast to a read-only looser type so we can
  // probe them safely without narrowing the union.
  const q = query as { file?: string; target?: string; entrypoints?: string[] };
  const candidate = q.file ?? q.target ?? q.entrypoints?.[0];
  if (!candidate || !isAbsolute(candidate)) return query;
  return { ...query, path: inferRootFromAbsoluteFile(candidate) };
}

export async function executeAnalyzeGraph(
  args: ToolExecutionArgs<AnalyzeGraphQuery>
): Promise<CallToolResult> {
  const graphCache = new Map<string, Promise<WalkResult>>();
  const getGraph = (
    path: string,
    excludeDir: string[],
    maxFiles: number,
    rustWorkspace: 'syntax' | 'cargo' = 'syntax'
  ): Promise<WalkResult> => {
    const key = JSON.stringify([
      path,
      [...excludeDir].sort(),
      maxFiles,
      rustWorkspace,
    ]);
    const existing = graphCache.get(key);
    if (existing) return existing;
    const pending = buildFileGraph(path, excludeDir, maxFiles, rustWorkspace);
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

          // Infer path from an absolute file field before security validation
          // so that callers can omit path when file is absolute.
          const resolvedQuery = inferPathIfMissing(parsed.data);

          const pathValidation = validateToolPath(
            resolvedQuery,
            TOOL_NAMES.LOCAL_ANALYZE_GRAPH
          );
          if (!pathValidation.isValid) {
            return createErrorResult(
              pathValidation.errorResult,
              resolvedQuery,
              { toolName: TOOL_NAMES.LOCAL_ANALYZE_GRAPH }
            ) as AnalyzeGraphOutput;
          }

          return analyzeGraph(
            {
              ...resolvedQuery,
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
