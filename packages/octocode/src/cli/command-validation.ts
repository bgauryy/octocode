import type { CLICommand, ParsedArgs } from './types.js';
import { c, bold, dim } from '../utils/colors.js';

// Flags accepted on every command, regardless of its own option list.
const GLOBAL_FLAGS = new Set([
  'json',
  'compact',
  'brief',
  'pretty',
  'minimal',
  'no-color',
  'help',
  'version',
]);

export function getAllowedOptionNames(command: CLICommand): Set<string> {
  return new Set([
    ...GLOBAL_FLAGS,
    ...(command.options ?? []).map(option => option.name),
  ]);
}

export function findUnknownOptions(
  command: CLICommand,
  args: ParsedArgs
): string[] {
  const allowed = getAllowedOptionNames(command);
  return Object.keys(args.options).filter(key => !allowed.has(key));
}

/** Levenshtein distance — used for "did you mean" suggestions. */
function editDistance(a: string, b: string): number {
  const rows = a.length + 1;
  const cols = b.length + 1;
  const dist = Array.from({ length: rows }, () =>
    new Array<number>(cols).fill(0)
  );
  for (let i = 0; i < rows; i++) dist[i][0] = i;
  for (let j = 0; j < cols; j++) dist[0][j] = j;
  for (let i = 1; i < rows; i++) {
    for (let j = 1; j < cols; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      dist[i][j] = Math.min(
        dist[i - 1][j] + 1,
        dist[i][j - 1] + 1,
        dist[i - 1][j - 1] + cost
      );
    }
  }
  return dist[a.length][b.length];
}

export function suggestFlag(
  unknown: string,
  allowed: Set<string>
): string | undefined {
  let best: string | undefined;
  let bestScore = Infinity;
  for (const name of allowed) {
    const score = editDistance(unknown, name);
    if (score < bestScore) {
      bestScore = score;
      best = name;
    }
  }
  // Only suggest a near-miss (typo), not an unrelated flag.
  return best && bestScore <= Math.max(2, Math.ceil(unknown.length / 3))
    ? best
    : undefined;
}

/**
 * Print an actionable error for unknown flags: name the offenders, suggest the
 * nearest valid flag, then list every flag the command accepts. Sets no exit
 * code — the caller owns that.
 */
export function printUnknownOptionError(
  command: CLICommand,
  unknown: string[]
): void {
  const allowed = getAllowedOptionNames(command);
  // List the command's own flags (not the always-implicit globals) for the menu.
  const ownFlags = [...allowed].filter(name => !GLOBAL_FLAGS.has(name)).sort();

  console.log();
  for (const flag of unknown) {
    const hint = suggestFlag(flag, allowed);
    const suffix = hint ? `  ${dim(`(did you mean --${hint}?)`)}` : '';
    console.log(
      `  ${c('red', '✗')} Unknown flag ${c('yellow', `--${flag}`)} for '${command.name}'${suffix}`
    );
  }
  console.log();
  console.log(`  ${bold(`Valid flags for ${command.name}:`)}`);
  console.log(`    ${ownFlags.map(name => c('cyan', `--${name}`)).join(' ')}`);
  console.log(`    ${dim('--json --compact --no-color')} ${dim('(global)')}`);
  console.log();
  console.log(
    `  ${dim('Run')} ${c('cyan', `${command.name} --help`)} ${dim('for full usage. For raw tool access:')} ${c('cyan', 'tools <name> --scheme')}`
  );
  console.log();
}
