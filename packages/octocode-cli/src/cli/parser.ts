import type { ParsedArgs } from './types.js';

const OPTIONS_WITH_VALUES = new Set([
  'ide',
  'method',
  'm',
  'output',
  'o',
  'hostname',
  'H',
  'git-protocol',
  'p',
  'type',
  't',
  'skill',
  'k',
  'local',
  'limit',
  'l',
  'targets',
  'mode',
  'model',
  'resume',
  'r',
  'id',
  'client',
  'c',
  'search',
  'category',
  'env',
  'config',
  'tool',
  'queries',
  'input',
  'responseCharLength',
  'responseCharOffset',
  'target',
  'backup-path',
]);

const BOOLEAN_OPTIONS = new Set([
  'help',
  'h',
  'version',
  'v',
  'force',
  'source',
  'json',
  'status',
  'dry-run',
  'installed',
  'repos',
  'skills',
  'logs',
  'all',
  'tools',
  'full',
  'direct',
  'lsp',
  'api',
  'list',
  'schema',
  'tools-context',
  'check',
  'rollback',
  'install',
  'yes',
  'y',
  'validate',
  'sync',
]);

const SINGLE_DASH_LONG_OPTIONS = new Set([
  'output',
  'responseCharLength',
  'responseCharOffset',
  'tool',
  'queries',
]);

function shouldConsumeNextValue(args: ParsedArgs, key: string): boolean {
  if (BOOLEAN_OPTIONS.has(key)) {
    return false;
  }

  if (OPTIONS_WITH_VALUES.has(key)) {
    return true;
  }

  return args.command === 'tool' || typeof args.options['tool'] === 'string';
}

export function parseArgs(argv: string[] = process.argv.slice(2)): ParsedArgs {
  const result: ParsedArgs = {
    command: null,
    args: [],
    options: {},
  };

  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];

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
    } else if (arg.startsWith('-') && !arg.startsWith('--') && arg.length > 2) {
      const normalized = arg.slice(1);
      const [key, value] = normalized.split('=');

      if (SINGLE_DASH_LONG_OPTIONS.has(key)) {
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
      } else {
        const flags = normalized;
        const lastFlag = flags[flags.length - 1];
        if (
          flags.length === 1 &&
          OPTIONS_WITH_VALUES.has(lastFlag) &&
          i + 1 < argv.length &&
          !argv[i + 1].startsWith('-')
        ) {
          result.options[lastFlag] = argv[i + 1];
          i++;
        } else {
          for (const flag of flags) {
            result.options[flag] = true;
          }
        }
      }
    } else if (arg.startsWith('-') && arg.length > 1) {
      const flags = arg.slice(1);
      const lastFlag = flags[flags.length - 1];
      if (
        flags.length === 1 &&
        OPTIONS_WITH_VALUES.has(lastFlag) &&
        i + 1 < argv.length &&
        !argv[i + 1].startsWith('-')
      ) {
        result.options[lastFlag] = argv[i + 1];
        i++;
      } else {
        for (const flag of flags) {
          result.options[flag] = true;
        }
      }
    } else if (!result.command) {
      if (typeof result.options['tool'] === 'string') {
        result.args.push(arg);
      } else {
        result.command = arg;
      }
    } else {
      result.args.push(arg);
    }

    i++;
  }

  if (!result.command && typeof result.options['tool'] === 'string') {
    result.command = 'tool';
    result.args = [result.options['tool'], ...result.args];
  }

  return result;
}

export function hasHelpFlag(args: ParsedArgs): boolean {
  return Boolean(args.options['help'] || args.options['h']);
}

export function hasVersionFlag(args: ParsedArgs): boolean {
  return Boolean(args.options['version'] || args.options['v']);
}
