import { contextUtils, type FilterPatchOptions } from '../contextUtils.js';

const DIFF_CONTEXT_LINES = 2;
const DIFF_TRIM_THRESHOLD_LINES = 30;

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
  const lines = patch.split('\n');
  if (lines.length <= DIFF_TRIM_THRESHOLD_LINES) return patch;

  const changedLineIndexes = lines
    .map((line, index) => (isChangedPatchLine(line) ? index : -1))
    .filter(index => index >= 0);

  if (changedLineIndexes.length === 0) return patch;

  const keep = new Set<number>();
  for (const [index, line] of lines.entries()) {
    if (line.startsWith('@@')) keep.add(index);
  }

  for (const index of changedLineIndexes) {
    const start = Math.max(0, index - DIFF_CONTEXT_LINES);
    const end = Math.min(lines.length - 1, index + DIFF_CONTEXT_LINES);
    for (let i = start; i <= end; i++) keep.add(i);
  }

  const trimmed: string[] = [];
  let omitted = false;

  for (const [index, line] of lines.entries()) {
    if (keep.has(index)) {
      trimmed.push(line);
      omitted = false;
    } else if (!omitted) {
      trimmed.push('...');
      omitted = true;
    }
  }

  const result = trimmed.join('\n');
  return result.length < patch.length ? result : patch;
}

function isChangedPatchLine(line: string): boolean {
  if (line.startsWith('+++') || line.startsWith('---')) return false;
  return line.startsWith('+') || line.startsWith('-');
}
