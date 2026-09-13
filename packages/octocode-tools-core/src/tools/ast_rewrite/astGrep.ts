import type { AstRewriteQuery } from './types.js';
import type { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';

/** ast-grep run uses exit 1 for a completed scan with no matches. */
export function scanSucceeded(
  result: Awaited<ReturnType<typeof spawnWithTimeout>>
): boolean {
  return (
    result.success ||
    (result.exitCode === 1 &&
      !result.error &&
      !result.timedOut &&
      !result.outputLimitExceeded &&
      result.stderr.trim() === '' &&
      result.stdout.trim() !== '' &&
      decodeMatches(result.stdout)?.length === 0)
  );
}

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
  metaVariables?: {
    single: Record<string, { text: string }>;
    multi: Record<string, Array<{ text: string }>>;
    transformed: Record<string, string>;
  };
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
    const metaVariables = match.metaVariables as
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
    if (metaVariables !== undefined && !validMetaVariables(metaVariables))
      return null;
    decoded.push(item as AstGrepJsonMatch);
  }
  return decoded;
}

function validTextRecord(value: unknown, multi: boolean): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  return Object.values(value).every(item => {
    const values = multi ? item : [item];
    return (
      Array.isArray(values) &&
      values.every(
        entry =>
          !!entry &&
          typeof entry === 'object' &&
          typeof (entry as Record<string, unknown>).text === 'string'
      )
    );
  });
}

function validMetaVariables(value: Record<string, unknown>): boolean {
  const transformed = value.transformed;
  return (
    validTextRecord(value.single, false) &&
    validTextRecord(value.multi, true) &&
    !!transformed &&
    typeof transformed === 'object' &&
    !Array.isArray(transformed) &&
    Object.values(transformed).every(item => typeof item === 'string')
  );
}

function inlineRule(query: AstRewriteQuery): Record<string, unknown> {
  if (query.ruleKind === 'pattern') throw new Error('Expected inline rule.');
  return {
    id: 'octocode-inline-rewrite',
    language: query.langType,
    severity: 'warning',
    message: 'Octocode inline structural rewrite',
    rule: query.rule,
    fix: query.fix,
    ...(query.constraints ? { constraints: query.constraints } : {}),
    ...(query.utils ? { utils: query.utils } : {}),
    ...(query.transform ? { transform: query.transform } : {}),
    ...(query.ruleKind === 'experimental'
      ? { rewriters: query.rewriters }
      : {}),
  };
}

export function buildArgs(query: AstRewriteQuery, target: string): string[] {
  const args =
    query.ruleKind === 'rule' || query.ruleKind === 'experimental'
      ? ['scan', '--inline-rules', JSON.stringify(inlineRule(query))]
      : [
          'run',
          '--pattern',
          query.pattern,
          '--rewrite',
          query.rewrite,
          '--lang',
          query.langType,
        ];
  args.push('--json=compact', '--color', 'never', '--threads', '1');
  for (const glob of query.include ?? []) args.push('--globs', glob);
  for (const glob of query.exclude ?? [])
    args.push('--globs', glob.startsWith('!') ? glob : `!${glob}`);
  args.push(target);
  return args;
}
