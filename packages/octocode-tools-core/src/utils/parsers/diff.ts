import {
  filterPatch as filterPatchNative,
  type FilterPatchOptions,
} from '@octocodeai/octocode-context-utils';

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
  return filterPatchNative(patch, options);
}

/**
 * Trim a raw unified diff to at most 2 context lines around each changed block.
 * Delegated to `filterPatch` with `trimContext: true`.
 */
export function trimDiffContext(patch: string): string {
  if (!patch) return '';
  return filterPatchNative(patch, { trimContext: true, contextLines: 2 });
}
