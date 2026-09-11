import { isAbsolute } from 'node:path';

import {
  buildFileGraph,
  type WalkResult,
} from '../../../graph/buildFileGraph.js';
import {
  createErrorResult,
  validateToolPath,
} from '../../../utils/file/toolHelpers.js';
import { executeWithToolBoundary } from '../../executionGuard.js';
import { AST_SEARCH_TOOL_NAME } from '@octocodeai/octocode-core/schema';
import { safeParseOrError } from '../../utils.js';
import { analyzeTopology } from './analyzeTopology.js';
import { inferRootFromAbsoluteFile } from './rootInference.js';
import {
  type TopologyAnalysisOutput,
  type TopologyAnalysisQuery,
  GraphAnalysisQuerySchema,
} from './scheme.js';

/**
 * When `path` is omitted, try to derive the repository root from the first
 * absolute file-like field in the query.  The walk stops at the nearest
 * Cargo manifest for Rust or `package.json`, matching the impl-level fallback in
 * `analyzeTopology()` so both call paths are consistent.
 */
function inferPathIfMissing(
  query: TopologyAnalysisQuery
): TopologyAnalysisQuery {
  if (query.path) return query;
  // `file`, `target`, and `entrypoints` are only present on specific
  // discriminated-union variants; cast to a read-only looser type so we can
  // probe them safely without narrowing the union.
  const q = query as { file?: string; target?: string; entrypoints?: string[] };
  const candidate = [q.file, q.target, ...(q.entrypoints ?? [])].find(
    value => value !== undefined && isAbsolute(value)
  );
  if (!candidate) return query;
  return {
    ...query,
    path: inferRootFromAbsoluteFile(candidate, query.rustWorkspace),
  };
}

export function createGraphAnalysisRunner() {
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

  return (query: TopologyAnalysisQuery) =>
    executeWithToolBoundary({
      toolName: AST_SEARCH_TOOL_NAME,
      query,
      contextMessage: 'astSearch topology execution failed',
      execute: async () => {
        const parsed = safeParseOrError<TopologyAnalysisQuery>(
          GraphAnalysisQuerySchema,
          query
        );
        if (parsed.ok === false) return parsed.error;

        // Infer path from an absolute file field before security validation
        // so that callers can omit path when file is absolute.
        const resolvedQuery = inferPathIfMissing(parsed.data);

        const pathValidation = validateToolPath(
          resolvedQuery,
          AST_SEARCH_TOOL_NAME
        );
        if (!pathValidation.isValid) {
          return createErrorResult(pathValidation.errorResult, resolvedQuery, {
            toolName: AST_SEARCH_TOOL_NAME,
          }) as TopologyAnalysisOutput;
        }

        return analyzeTopology(
          {
            ...resolvedQuery,
            path: pathValidation.sanitizedPath,
          },
          { getGraph }
        );
      },
    });
}
