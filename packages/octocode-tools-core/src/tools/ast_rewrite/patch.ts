function splitLines(value: string): string[] {
  const lines = value.split('\n');
  if (lines.at(-1) === '') lines.pop();
  return lines;
}

function hunkCount(lines: string[]): number {
  return lines.length;
}

/** Create a valid, single-hunk unified diff with at most three context lines. */
export function createUnifiedPatch(
  path: string,
  before: string,
  after: string
): string {
  if (before === after) return '';
  const oldLines = splitLines(before);
  const newLines = splitLines(after);
  let prefix = 0;
  while (
    prefix < oldLines.length &&
    prefix < newLines.length &&
    oldLines[prefix] === newLines[prefix]
  ) {
    prefix += 1;
  }
  let suffix = 0;
  while (
    suffix < oldLines.length - prefix &&
    suffix < newLines.length - prefix &&
    oldLines[oldLines.length - 1 - suffix] ===
      newLines[newLines.length - 1 - suffix]
  ) {
    suffix += 1;
  }

  const contextStart = Math.max(0, prefix - 3);
  const oldEnd = Math.min(oldLines.length, oldLines.length - suffix + 3);
  const leading = oldLines.slice(contextStart, prefix);
  const removed = oldLines.slice(prefix, oldLines.length - suffix);
  const added = newLines.slice(prefix, newLines.length - suffix);
  const trailing = oldLines.slice(oldLines.length - suffix, oldEnd);
  const oldHunk = [...leading, ...removed, ...trailing];
  const newHunk = [...leading, ...added, ...trailing];
  const body = [
    ...leading.map(line => ` ${line}`),
    ...removed.map(line => `-${line}`),
    ...added.map(line => `+${line}`),
    ...trailing.map(line => ` ${line}`),
  ];
  return [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -${contextStart + 1},${hunkCount(oldHunk)} +${contextStart + 1},${hunkCount(newHunk)} @@`,
    ...body,
    '',
  ].join('\n');
}
