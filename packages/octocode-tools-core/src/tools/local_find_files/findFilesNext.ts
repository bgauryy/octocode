import type { LocalFindFilesEntry } from '@octocodeai/octocode-core/types';

type FindFilesNextCall = {
  tool: 'localGetFileContent' | 'local.tree';
  query: Record<string, unknown>;
  why: string;
  confidence?: 'exact' | 'high' | 'medium' | 'low';
};

export type FindFilesNextMap = {
  fetch?: FindFilesNextCall;
  viewStructure?: FindFilesNextCall;
};

/**
 * Build per-result continuation hints for a local files response: point the
 * agent at exact content for the first matched file, or into the first matched
 * directory when the page is all directories. `nextPage` (pagination) is added
 * separately by the caller.
 *
 * The queries carry the entry's absolute path, which the response
 * path-relativizer deliberately leaves untouched inside `next` (see
 * SKIP_TRAVERSAL_KEYS in utils/response/pathRelativize.ts), so the hint stays
 * directly callable with no manual path conversion. Meta-free by construction:
 * queries are built from the entry path alone, never spread from the caller
 * query, so no auto-filled goal/reasoning can leak in.
 */
export function buildFindFilesNextMap(
  files: readonly LocalFindFilesEntry[]
): FindFilesNextMap | undefined {
  const firstFile = files.find(entry => entry.type === 'file');
  if (firstFile) {
    return {
      fetch: {
        tool: 'localGetFileContent',
        query: { path: firstFile.path, minify: 'standard' },
        why: 'Read the first matched file (minify:"symbols" for a skeleton, minify:"none" for exact bytes).',
        confidence: 'exact',
      },
    };
  }

  const firstDir = files.find(entry => entry.type === 'directory');
  if (firstDir) {
    return {
      viewStructure: {
        tool: 'local.tree',
        query: { path: firstDir.path },
        why: 'Orient inside the first matched directory before reading files.',
        confidence: 'exact',
      },
    };
  }

  return undefined;
}
