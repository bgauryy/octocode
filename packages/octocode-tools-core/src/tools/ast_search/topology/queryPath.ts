import { isAbsolute } from 'node:path';
import { inferRootFromAbsoluteFile } from './rootInference.js';
import type {
  TopologyAnalysisOutput,
  TopologyAnalysisQuery,
} from './analysisTypes.js';

export function resolveTopologyQueryPath(
  rawQuery: TopologyAnalysisQuery
):
  | { query: TopologyAnalysisQuery & { path: string } }
  | { error: TopologyAnalysisOutput } {
  let path = rawQuery.path;
  if (!path) {
    const candidate = [
      rawQuery.file,
      rawQuery.target,
      ...(rawQuery.entrypoints ?? []),
    ].find(value => value !== undefined && isAbsolute(value));
    if (candidate)
      path = inferRootFromAbsoluteFile(candidate, rawQuery.rustWorkspace);
  }
  if (path) return { query: { ...rawQuery, path } };
  return {
    error: {
      status: 'error',
      error:
        'path is required — or provide an absolute file path to infer its nearest Cargo.toml (Rust) or package.json root',
      errorCode: 'invalidGraphQuery',
      operation: rawQuery.operation,
      path: '',
      results: [],
    },
  };
}
