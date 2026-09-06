import { contextUtils } from '../contextUtils.js';
import type { FilterPatchOptions } from '@octocodeai/octocode-engine';
import { splitLines } from '../core/lines.js';

const DIFF_CONTEXT_LINES = 2;

/** Max diff lines carried in a {@link DiffPreview} before the rest are summarized. */
export const DIFF_PREVIEW_MAX_LINES = 20;

/**
 * A render-ready slice of a patch: the first {@link DIFF_PREVIEW_MAX_LINES}
 * lines plus a count of how many were withheld. Consumers (e.g. the CLI) only
 * colorize `lines` and print `moreCount` — they do no splitting or counting, so
 * the trailing-newline off-by-one lives in exactly one place.
 */
export interface DiffPreview {
  lines: string[];
  moreCount: number;
}

/**
 * Build a {@link DiffPreview} from a raw patch string. Splitting is newline-safe
 * via {@link splitLines}, so a patch that ends in a newline neither inflates
 * `moreCount` nor surfaces a phantom blank line.
 */
export function buildDiffPreview(
  patch: string | undefined,
  maxLines: number = DIFF_PREVIEW_MAX_LINES
): DiffPreview {
  const all = patch ? splitLines(patch) : [];
  return {
    lines: all.slice(0, maxLines),
    moreCount: Math.max(0, all.length - maxLines),
  };
}

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

export function trimDiffContext(patch: string): string {
  if (!patch) return '';
  return contextUtils.filterPatch(patch, {
    trimContext: true,
    contextLines: DIFF_CONTEXT_LINES,
  });
}
