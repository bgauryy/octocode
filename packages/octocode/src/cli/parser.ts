import type { ParsedArgs } from './types.js';

// Only options the Node side itself reads need value-consumption here — the
// `skill` and `scheme` commands' value flags. Everything else is forwarded to
// the native binary as raw argv, which owns its own parsing.
const OPTIONS_WITH_VALUES = new Set([
  'add',
  'mode',
  'path',
  'platform',
  'project-dir',
  'select',
  'view',
]);

function shouldConsumeNextValue(_args: ParsedArgs, key: string): boolean {
  return OPTIONS_WITH_VALUES.has(key);
}

export function parseArgs(argv: string[] = process.argv.slice(2)): ParsedArgs {
  const result: ParsedArgs = {
    command: null,
    args: [],
    options: {},
    raw: [...argv],
  };

  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];

    // Bare "--" is the conventional npm/yarn/pnpm arg separator (e.g.
    // `yarn start -- search x --json`). Skip it; keep parsing what follows as
    // normal so flags after it still work.
    if (arg === '--') {
      i++;
      continue;
    }

    if (arg.startsWith('--')) {
      const [key, value] = arg.slice(2).split('=');
      if (value !== undefined) {
        result.options[key] = value;
      } else if (
        shouldConsumeNextValue(result, key) &&
        i + 1 < argv.length &&
        !argv[i + 1].startsWith('-')
      ) {
        result.options[key] = argv[i + 1];
        i++;
      } else {
        result.options[key] = true;
      }
    } else if (!result.command) {
      result.command = arg;
    } else {
      result.args.push(arg);
    }

    i++;
  }

  return result;
}

export function hasHelpFlag(args: ParsedArgs): boolean {
  return Boolean(args.options['help']);
}

export function hasVersionFlag(args: ParsedArgs): boolean {
  return Boolean(args.options['version']);
}
