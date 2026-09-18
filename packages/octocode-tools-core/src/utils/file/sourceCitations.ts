import type { ContentWindow } from './contentPagination.js';

/** Source coordinates for consecutive runs of lines in the returned page. */
export interface SourceLineRange {
  start: number;
  end: number;
}

export function sourcePageRanges(
  window: ContentWindow,
  selectedSourceLines?: readonly number[]
): SourceLineRange[] {
  const ranges: SourceLineRange[] = [];
  for (
    let viewLine = window.firstViewLine;
    viewLine <= window.lastViewLine;
    viewLine++
  ) {
    const line = selectedSourceLines
      ? selectedSourceLines[viewLine - 1]
      : viewLine;
    // A missing coordinate invalidates the map; never label a guessed position.
    if (line === undefined) return [];
    const previous = ranges.at(-1);
    if (previous && previous.end + 1 === line) previous.end = line;
    else ranges.push({ start: line, end: line });
  }
  return ranges;
}

/** Decorate only the text presentation; structured content stays byte-exact. */
export function renderSourceLines(
  content: string,
  value: unknown
): string | undefined {
  if (!Array.isArray(value) || value.length === 0) return undefined;
  const records = content.match(/[^\n]*\n|[^\n]+$/g) ?? [];
  const output: string[] = [];
  let index = 0;
  for (const range of value) {
    if (
      !range ||
      !Number.isSafeInteger(range.start) ||
      !Number.isSafeInteger(range.end) ||
      range.start < 1 ||
      range.end < range.start ||
      range.end - range.start >= records.length
    ) {
      return undefined;
    }
    for (let line = range.start; line <= range.end; line++) {
      const record = records[index++];
      if (record === undefined) return undefined;
      output.push(`${line}: ${record}`);
    }
  }
  return index === records.length ? output.join('') : undefined;
}
