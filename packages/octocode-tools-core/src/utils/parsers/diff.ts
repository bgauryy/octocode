import { contextUtils, type FilterPatchOptions } from '../contextUtils.js';

const DIFF_CONTEXT_LINES = 2;

export function filterPatch(
  patch: string,
  additions?: number[],
  deletions?: number[]
): string {
  if (!patch) return '';

  if (additions === undefined && deletions === undefined) {
    return patch;
  }

  const options: FilterPatchOptions = { additions, deletions };
  return contextUtils.filterPatch(patch, options);
}

/**
 * Trim a raw unified diff to at most 2 context lines around each changed block.
 */
export function trimDiffContext(patch: string): string {
  if (!patch) return '';
  return contextUtils.filterPatch(patch, {
    trimContext: true,
    contextLines: DIFF_CONTEXT_LINES,
  });
}
