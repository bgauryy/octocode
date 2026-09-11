import type { ParsedArgs } from './types.js';

const OPTIONS_WITH_VALUES = new Set([
  'ide',
  'method',
  'hostname',
  'git-protocol',
  'path',
  'github',
  'branch',
  'add',
  'platform',
  'project-dir',
  'local',
  'limit',
  'depth',
  'mode',
  'search',
  'queries',
  'format',
  'view',
  'backup-path',
  'query',
  'file',
  'pr',
  'page',
  'page-size',
  'items-per-page',
  'char-offset',
  'char-length',
  'line',
  'context-lines',
  'kind',
  'name',
  'min-depth',
  'max-depth',
  'match-length',
  'max-files',
  'match-page',
  'owner',
  'repo',
  'size',
  'start-line',
  'end-line',
  'max-matches',
]);

const BOOLEAN_OPTIONS = new Set([
  'help',
  'version',
  'yaml',
  'text',
  'force',
  'json',
  'status',
  'dry-run',
  'full',
  'scheme',
  'brief',
  'compact',
  'pretty',
  'minimal',
  'no-color',
  'raw',
  'check',
  'rollback',
  'update',
  'install',
  'yes',
  'all',
  'empty',
  'force-refresh',
  'tree',
  // skill subcommand flags.
  'workspace',
  'repo',
  'keep',
  'fix',
  'no-env',
  'global',
]);

function shouldConsumeNextValue(args: ParsedArgs, key: string): boolean {
  if (BOOLEAN_OPTIONS.has(key)) {
    return false;
  }

  if (OPTIONS_WITH_VALUES.has(key)) {
    return true;
  }

  return args.command === 'tools';
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
