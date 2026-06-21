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
  'queries',
  'format',
  'input',
  'responseCharLength',
  'responseCharOffset',
  'target',
  'backup-path',
  'query',
  'state',
  'author',
  'label',
  'base',
  'file',
  'pr',
  'page',
  'page-size',
  'char-offset',
  'char-length',
  'symbol',
  'line',
  'workspace-root',
  'context-lines',
  'context',
  'ext',
  'kind',
  'entry',
  'name',
  'path-pattern',
  'regex',
  'min-depth',
  'max-depth',
  'modified-within',
  'modified-before',
  'accessed-within',
  'size-greater',
  'size-less',
  'permissions',
  'include',
  'exclude',
  'exclude-dir',
  'filename',
  'match-length',
  'match-window',
  'max-matches-per-file',
  'max-files',
  'match-page',
  'topic',
  'language',
  'owner',
  'repo',
  'stars',
  'forks',
  'good-first-issues',
  'license',
  'created',
  'updated',
  'since',
  'until',
  'size',
  'match',
  'content-type',
  'sort',
  'visibility',
  'archived',
  'match-string',
  'start-line',
  'end-line',
  'pattern',
  'rule',
  'max-matches',
  'extract',
  'min-length',
  'max-entries',
  'scan-offset',
  'lang',
  'materialize',
  'minify',
]);

const BOOLEAN_OPTIONS = new Set([
  'help',
  'version',
  'force',
  'source',
  'json',
  'concise',
  'status',
  'stats',
  'context',
  'dry-run',
  'full',
  'direct',
  'list',
  'scheme',
  'compact',
  'no-color',
  'reveal',
  'raw',
  'check',
  'rollback',
  'install',
  'yes',
  'validate',
  'verbose',
  'empty',
  'executable',
  'readable',
  'writable',
  'details',
  'show-modified',
  'fixed-string',
  'perl-regex',
  'case-insensitive',
  'case-sensitive',
  'whole-word',
  'invert-match',
  'hidden',
  'no-ignore',
  'files-only',
  'dirs-only',
  'files-without-match',
  'reverse',
  'multiline',
  'multiline-dotall',
  'sort-reverse',
  'count-lines',
  'count-matches',
  'only-matching',
  'diff',
  'match-regex',
  'match-case-sensitive',
  'full-content',
  'force-refresh',
  'strings',
  'decompress',
  'inspect',
  'offsets',
  'repos',
  'all',
  'pcre2',
]);

function shouldConsumeNextValue(args: ParsedArgs, key: string): boolean {
  if (key === 'source' && args.command === 'find') {
    return true;
  }

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
  };

  let i = 0;
  while (i < argv.length) {
    const arg = argv[i];

    // Bare "--" is the conventional npm/yarn/pnpm arg separator (e.g.
    // `yarn start -- pkg x --json`). Skip it; keep parsing what follows as
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
