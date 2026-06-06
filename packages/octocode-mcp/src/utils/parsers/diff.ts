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
