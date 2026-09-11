import type { AstRewriteQuery } from './types.js';

export interface AstGrepJsonMatch {
  file: string;
  text: string;
  replacement: string;
  range: {
    byteOffset: { start: number; end: number };
    start: { line: number; column: number };
    end: { line: number; column: number };
  };
  replacementOffsets?: { start: number; end: number };
}

function validPosition(
  value: unknown
): value is { line: number; column: number } {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Record<string, unknown>;
  return (
    Number.isInteger(candidate.line) &&
    Number(candidate.line) >= 0 &&
    Number.isInteger(candidate.column) &&
    Number(candidate.column) >= 0
  );
}

export function decodeMatches(stdout: string): AstGrepJsonMatch[] | null {
  let value: unknown;
  try {
    value = JSON.parse(stdout || '[]');
  } catch {
    return null;
  }
  if (!Array.isArray(value)) return null;
  const decoded: AstGrepJsonMatch[] = [];
  for (const item of value) {
    if (!item || typeof item !== 'object') return null;
    const match = item as Record<string, unknown>;
    const range = match.range as Record<string, unknown> | undefined;
    const bytes = range?.byteOffset as Record<string, unknown> | undefined;
    const replacementOffsets = match.replacementOffsets as
      Record<string, unknown> | undefined;
    if (
      typeof match.file !== 'string' ||
      typeof match.text !== 'string' ||
      typeof match.replacement !== 'string' ||
      !range ||
      !bytes ||
      !Number.isInteger(bytes.start) ||
      !Number.isInteger(bytes.end) ||
      !validPosition(range.start) ||
      !validPosition(range.end) ||
      (replacementOffsets !== undefined &&
        (!Number.isInteger(replacementOffsets.start) ||
          !Number.isInteger(replacementOffsets.end)))
    ) {
      return null;
    }
    decoded.push(item as AstGrepJsonMatch);
  }
  return decoded;
}

export function buildArgs(query: AstRewriteQuery, target: string): string[] {
  const args = [
    'run',
    '--pattern',
    query.pattern,
    '--rewrite',
    query.rewrite,
    '--lang',
    query.langType,
    '--json=compact',
    '--color',
    'never',
    '--threads',
    '1',
  ];
  for (const glob of query.include ?? []) args.push('--globs', glob);
  for (const glob of query.exclude ?? [])
    args.push('--globs', glob.startsWith('!') ? glob : `!${glob}`);
  args.push(target);
  return args;
}
