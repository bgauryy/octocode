import type { ParsedArgs } from '../commands/args.js';
import { die } from '../command-output.js';
import { BOOLEAN_FLAGS, NUMERIC_FLAGS, RETENTION_DAY_FLAGS, VALUE_REQUIRED_FLAGS } from '../command-parser.js';
import { COMMAND_ROUTES } from '../commands/routes.js';

/**
 * Reject silently-coerced flag values: non-integer numeric flags and
 * value-required flags that got boolean-coerced (`--query --smart`). Runs for
 * every command so bad input fails loudly instead of falling back to a default.
 */
export function validateFlagValues(args: ParsedArgs): void {
  for (const key of Object.keys(args)) {
    if (key === '_') continue;
    const value = args[key];
    if (value === false && !BOOLEAN_FLAGS.has(key)) {
      die(`--no-${key.replace(/_/g, '-')} is invalid because --${key.replace(/_/g, '-')} expects a value`);
    } else if (BOOLEAN_FLAGS.has(key) && typeof value !== 'boolean') {
      die(`--${key.replace(/_/g, '-')} expects a boolean (true/false, yes/no, or 1/0)`, { got: String(value) });
    } else if (NUMERIC_FLAGS.has(key)) {
      const n = typeof value === 'string' ? Number(value) : NaN;
      if (value === true || !Number.isInteger(n)) {
        die(`--${key.replace(/_/g, '-')} expects an integer`, { got: value === true ? 'flag with no value' : String(value) });
      }
      if (RETENTION_DAY_FLAGS.has(key) && (n < 1 || n > 3650)) {
        die(`--${key.replace(/_/g, '-')} must be in 1..3650`, { got: n });
      }
    } else if (VALUE_REQUIRED_FLAGS.has(key) && value === true) {
      die(`--${key.replace(/_/g, '-')} expects a value (it was followed by another flag)`);
    }
  }
}

export function extractGlobalDb(argv: string[]): { dbPath: string | null; dbScope: string | null; filtered: string[] } {
  let dbPath: string | null = null;
  let dbScope: string | null = null;
  const filtered: string[] = [];
  let i = 0;
  while (i < argv.length) {
    const token = argv[i]!;
    const equalsIndex = token.indexOf('=');
    const flag = equalsIndex >= 0 ? token.slice(0, equalsIndex) : token;
    if (flag === '--db' || flag === '--db-scope') {
      const inlineValue = equalsIndex >= 0 ? token.slice(equalsIndex + 1) : undefined;
      const value = inlineValue ?? argv[i + 1];
      if (!value || (inlineValue === undefined && value.startsWith('--'))) {
        die(flag === '--db' ? '--db expects a path' : '--db-scope expects a value');
      }
      if (flag === '--db') dbPath = value;
      else dbScope = value;
      i += inlineValue === undefined ? 2 : 1;
    } else {
      filtered.push(argv[i]!); i++;
    }
  }
  return { dbPath, dbScope, filtered };
}

export const SINGLE_COMMANDS = new Set(['query', 'attend', 'schema', 'status']);
export const UNKNOWN_COMMAND = '__unknown__';

export function normalizeToken(value: string | undefined): string | undefined {
  return value?.replace(/_/g, '-');
}

export function selectCommand(argv: string[]): { command: string | undefined; rest: string[] } {
  const [firstRaw, secondRaw, thirdRaw, ...tail] = argv;
  const first = normalizeToken(firstRaw);
  if (!first) return { command: undefined, rest: [] };
  if (first.startsWith('-')) {
    // Tolerate a leading global flag (e.g. `--compact workspace status`): pull
    // it off, re-select on the remainder, and re-append it so parseArgs still
    // sees it. Without this the whole argv was mis-read as one unknown command.
    if (first === '--compact' && argv.length > 1) {
      const sel = selectCommand(argv.slice(1));
      if (sel.command && sel.command !== UNKNOWN_COMMAND) {
        return { command: sel.command, rest: [...sel.rest, '--compact'] };
      }
    }
    return argv.every((arg) => arg === '--compact')
      ? { command: undefined, rest: argv }
      : { command: UNKNOWN_COMMAND, rest: argv };
  }

  const second = normalizeToken(secondRaw);
  if (first === 'hook' && second === 'run') {
    return { command: 'hook-run', rest: thirdRaw ? [thirdRaw, ...tail] : tail };
  }
  if (first === 'hooks' && second) {
    if (second === 'install') return { command: 'hooks-install', rest: thirdRaw ? [thirdRaw, ...tail] : tail };
    if (second === 'check') return { command: 'hooks-install', rest: ['--check', ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
    if (second === 'remove') return { command: 'hooks-install', rest: ['--remove', ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
  }
  if (first === 'schema') {
    return { command: 'schema', rest: secondRaw ? [secondRaw, ...(thirdRaw ? [thirdRaw, ...tail] : tail)] : [] };
  }

  if (second) {
    const route = COMMAND_ROUTES[`${first} ${second}`];
    if (route) return { command: route.command, rest: [...(route.action ? ['--action', route.action] : []), ...(thirdRaw ? [thirdRaw, ...tail] : tail)] };
  }

  if (SINGLE_COMMANDS.has(first)) {
    return { command: first, rest: secondRaw ? [secondRaw, ...(thirdRaw ? [thirdRaw, ...tail] : tail)] : [] };
  }

  return { command: UNKNOWN_COMMAND, rest: argv };
}
