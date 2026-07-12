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
  'add',
  'platform',
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
  'op',
  'view',
  'content-view',
  'backup-path',
  'query',
  'state',
  'author',
  'label',
  'base',
  'head',
  'file',
  'pr',
  'page',
  'page-size',
  'items-per-page',
  'char-offset',
  'char-length',
  'symbol',
  'line',
  'workspace-root',
  'uri',
  'order',
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
  'max-files',
  'ranking-profile',
  'match-page',
  'topic',
  'owner',
  'repo',
  'stars',
  'forks',
  'good-first-issues',
  'license',
  'created',
  'updated',
  'closed',
  'merged-at',
  'since',
  'until',
  'size',
  'match',
  'sort',
  'visibility',
  'archived',
  'match-string',
  'start-line',
  'end-line',
  'pattern',
  'rule',
  'max-matches',
  'lang',
  'materialize',
  'review-mode',
  'file-page',
  'comment-page',
  'commit-page',
  'base-ref',
  'head-ref',
  'intent',
  'facets',
  'proof',
  'proof-limit',
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
  'update',
  'install',
  'yes',
  'all',
  'validate',
  'verbose',
  'empty',
  'executable',
  'readable',
  'writable',
  'details',
  'show-modified',
  'fixed',
  'case-insensitive',
  'case-sensitive',
  'whole-word',
  'invert-match',
  'hidden',
  'no-ignore',
  'files-only',
  'files-without-match',
  'multiline',
  'multiline-dotall',
  'sort-reverse',
  'debug-ranking',
  'count-lines',
  'count-matches',
  'count',
  'only-matching',
  'match-regex',
  'match-case-sensitive',
  'full-content',
  'force-refresh',
  'patches',
  'tree',
  'include-sizes',
  'include-packets',
  'include-facts',
  'include-edges',
  'offsets',
  'repos',
  'all',
  'pcre2',
]);

function shouldConsumeNextValue(args: ParsedArgs, key: string): boolean {
  if (key === 'source' && args.command === 'search') {
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
