import type { ParsedArgs } from './types.js';

const OPTIONS_WITH_VALUES = new Set([
  'ide',
  'method',
  'output',
  'hostname',
  'git-protocol',
  'path',
  'github',
  'branch',
  'type',
  'skill',
  'local',
  'limit',
  'depth',
  'targets',
  'mode',
  'model',
  'resume',
  'id',
  'content',
  'search',
  'tool',
  'queries',
  'format',
  'input',
  'responseCharLength',
  'responseCharOffset',
  'target',
  'backup-path',
  // Research command options (get, search, tree, pr)
  'query',
  'state',
  'author',
  'label',
  'base',
  'file',
  'pr',
  'page',
  'page-size',
  'match-string',
  'start-line',
  'end-line',
]);

const BOOLEAN_OPTIONS = new Set([
  'help',
  'version',
  'force',
  'source',
  'json',
  'status',
  'stats',
  'dry-run',
  'skills',
  'full',
  'direct',
  'list',
  'schema',
  'tools-context',
  'agent',
  'compact',
  'no-color',
  'reveal',
  'raw',
  'check',
  'rollback',
  'install',
  'yes',
  'validate',
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
    } else if (arg.startsWith('-') && arg.length > 1) {
      // Single-char flags (e.g. -v, -h, -j, -f)
      const flags = arg.slice(1);
      for (const flag of flags) {
        result.options[flag] = true;
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
