interface PatchLine {
  originalLineNumber: number | null;
  newLineNumber: number | null;
  content: string;
  type: 'context' | 'addition' | 'deletion';
}

function parsePatch(patch: string): PatchLine[] {
  const lines = patch.split('\n');
  const result: PatchLine[] = [];
  let originalLine = 0;
  let newLine = 0;

  for (const line of lines) {
    if (line.startsWith('@@')) {
      const match = line.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
      if (match && match[1] && match[2]) {
        originalLine = parseInt(match[1], 10) - 1;
        newLine = parseInt(match[2], 10) - 1;
      }
      continue;
    }

    if (line.startsWith('+')) {
      newLine++;
      result.push({
        originalLineNumber: null,
        newLineNumber: newLine,
        content: line,
        type: 'addition',
      });
    } else if (line.startsWith('-')) {
      originalLine++;
      result.push({
        originalLineNumber: originalLine,
        newLineNumber: null,
        content: line,
        type: 'deletion',
      });
    } else if (!line.startsWith('\\')) {
      originalLine++;
      newLine++;
      result.push({
        originalLineNumber: originalLine,
        newLineNumber: newLine,
        content: line,
        type: 'context',
      });
    }
  }

  return result;
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

  const parsed = parsePatch(patch);

  const addSet = additions !== undefined ? new Set(additions) : null;
  const delSet = deletions !== undefined ? new Set(deletions) : null;

  const filteredLines = parsed.filter(line => {
    if (line.type === 'addition' && line.newLineNumber !== null) {
      return addSet === null || addSet.has(line.newLineNumber);
    }
    if (line.type === 'deletion' && line.originalLineNumber !== null) {
      return delSet === null || delSet.has(line.originalLineNumber);
    }
    if (line.type === 'context') {
      return (
        addSet === null || addSet.size > 0 || delSet === null || delSet.size > 0
      );
    }
    return false;
  });

  if (filteredLines.length === 0) return '';

  return filteredLines
    .map(line => {
      const lineNum =
        line.type === 'addition'
          ? `+${line.newLineNumber}`
          : line.type === 'deletion'
            ? `-${line.originalLineNumber}`
            : ` ${line.newLineNumber}`;
      return `${lineNum}: ${line.content.substring(1)}`;
    })
    .join('\n');
}

const DIFF_CONTEXT_LINES = 2;

/**
 * Trim a raw unified diff to at most DIFF_CONTEXT_LINES context lines
 * around each changed block. Changed lines (+/-) and hunk headers (@@)
 * are always preserved. Only pure context lines are trimmed.
 *
 * Activates only when the diff is long enough to benefit (> 30 lines total).
 * Returns the original patch unchanged if it is already short or has no
 * context to trim.
 */
export function trimDiffContext(patch: string): string {
  if (!patch) return '';

  const lines = patch.split('\n');
  if (lines.length <= 30) return patch;

  const changed = new Set<number>();
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l !== undefined && (l.startsWith('+') || l.startsWith('-'))) {
      changed.add(i);
    }
  }

  if (changed.size === 0) return patch;

  const keep = new Set<number>();
  for (const ci of changed) {
    for (
      let j = Math.max(0, ci - DIFF_CONTEXT_LINES);
      j <= Math.min(lines.length - 1, ci + DIFF_CONTEXT_LINES);
      j++
    ) {
      keep.add(j);
    }
  }

  const result: string[] = [];
  let prevKept = true;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line === undefined) continue;
    if (line.startsWith('@@')) {
      result.push(line);
      prevKept = true;
      continue;
    }
    if (keep.has(i)) {
      result.push(line);
      prevKept = true;
    } else {
      if (prevKept) result.push('...');
      prevKept = false;
    }
  }

  const trimmed = result.join('\n');
  return trimmed.length < patch.length ? trimmed : patch;
}
