import type { DirectoryEntry } from './structureFilters.js';

type ViewStructureNextCall = {
  tool: 'localGetFileContent' | 'localViewStructure';
  query: Record<string, unknown>;
  why: string;
  confidence?: 'exact' | 'high' | 'medium' | 'low';
};

export type ViewStructureNextMap = {
  fetch?: ViewStructureNextCall;
  viewDeeper?: ViewStructureNextCall;
};

/**
 * Build per-result continuation hints for a localViewStructure response: read
 * the first listed file, and/or descend into the first listed subdirectory.
 * `nextPage` (pagination) is added separately by the caller.
 *
 * Queries carry each entry's absolute path, which the response
 * path-relativizer deliberately leaves untouched inside `next` (see
 * SKIP_TRAVERSAL_KEYS in utils/response/pathRelativize.ts), so hints stay
 * directly callable with no manual path conversion. Meta-free by construction:
 * queries are built from the entry path alone, never spread from the caller
 * query, so no auto-filled goal/reasoning can leak in.
 */
export function buildViewStructureNextMap(
  entries: readonly DirectoryEntry[]
): ViewStructureNextMap | undefined {
  const next: ViewStructureNextMap = {};

  const firstFile = entries.find(entry => entry.type === 'file' && entry.path);
  if (firstFile?.path) {
    next.fetch = {
      tool: 'localGetFileContent',
      query: { path: firstFile.path, minify: 'standard' },
      why: 'Read the first listed file (minify:"symbols" for a skeleton, minify:"none" for exact bytes).',
      confidence: 'exact',
    };
  }

  const firstDir = entries.find(
    entry => entry.type === 'directory' && entry.path
  );
  if (firstDir?.path) {
    next.viewDeeper = {
      tool: 'localViewStructure',
      query: { path: firstDir.path },
      why: 'Descend into the first listed subdirectory.',
      confidence: 'exact',
    };
  }

  return next.fetch || next.viewDeeper ? next : undefined;
}
