import {
  ALLOWED_COMMANDS,
  DANGEROUS_PATTERNS,
  PATTERN_DANGEROUS_PATTERNS,
} from './securityConstants.js';
import { securityRegistry } from './registry.js';

export function normalizeCommandName(command: string): string {
  if (!command || typeof command !== 'string') return command;
  const lastSep = Math.max(command.lastIndexOf('/'), command.lastIndexOf('\\'));
  const base = lastSep >= 0 ? command.slice(lastSep + 1) : command;
  return base.replace(/\.exe$/i, '');
}

const RG_ALLOWED_FLAGS = new Set([
  '-F',
  '-P',
  '-s',
  '-i',
  '-S',
  '--no-unicode',
  '-w',
  '-v',
  '-a',
  '--binary',
  '-L',
  '-n',
  '--line-number',
  '--column',
  '-l',
  '--files-without-match',
  '--count-matches',
  '-c',
  '--no-ignore',
  '--hidden',
  '-U',
  '--multiline-dotall',
  '--json',
  '--stats',
  '--no-mmap',
  '--no-messages',
  '-x',
  '--passthru',
  '--debug',
]);

const RG_ALLOWED_SHORT_FLAGS = new Set(
  [...RG_ALLOWED_FLAGS].filter(f => /^-[a-zA-Z]$/.test(f)).map(f => f[1]!)
);

const RG_ALLOWED_FLAGS_WITH_VALUES = new Set([
  '-g',
  '--glob',
  '--include',
  '--exclude',
  '--exclude-dir',
  '-A',
  '-B',
  '-C',
  '-m',
  '-t',
  '--type',
  '-T',
  '--type-not',
  '--type-add',
  '-j',
  '--threads',
  '--sort',
  '--sortr',
  '--max-filesize',
  '-E',
  '--encoding',
  '--color',
]);

const FIND_DISALLOWED_OPERATORS = new Set([
  '-delete',
  '-exec',
  '-execdir',
  '-ok',
  '-okdir',
  '-printf',
  '-fprintf',
  '-fprint',
  '-fprint0',
  '-fls',
  '-ls',
]);

const GIT_ALLOWED_SUBCOMMANDS = new Set(['clone', 'sparse-checkout']);

const GIT_CLONE_ALLOWED_FLAGS = new Set([
  '--depth',
  '--single-branch',
  '--branch',
  '--filter',
  '--sparse',
  '--no-checkout',
  '--quiet',
  '-q',
  '-c',
  '--',
]);

const GIT_SPARSE_CHECKOUT_ALLOWED_ACTIONS = new Set([
  'init',
  'set',
  'add',
  'list',
  'disable',
]);

const GIT_SPARSE_CHECKOUT_ALLOWED_FLAGS = new Set([
  '--cone',
  '--no-cone',
  '--',
]);

const GIT_SAFE_CONFIG_KEYS = new Set([
  'advice.detachedHead',
  'core.autocrlf',
  'core.sparseCheckout',
  'http.extraHeader',
  'http.followRedirects',
  'http.userAgent',
  'http.version',
]);

const GIT_BLOCKED_URL_PROTOCOLS = ['file://', 'git://', 'http://'] as const;

function validateGitConfigKeyValue(keyValue: string): string | null {
  const eqIndex = keyValue.indexOf('=');
  const key = eqIndex >= 0 ? keyValue.substring(0, eqIndex) : keyValue;
  if (!GIT_SAFE_CONFIG_KEYS.has(key)) {
    return `git config key '${key}' is not allowed via -c`;
  }
  return null;
}

function validateGitCloneUrl(url: string): string | null {
  for (const protocol of GIT_BLOCKED_URL_PROTOCOLS) {
    if (url.startsWith(protocol)) {
      return `git clone URL protocol '${protocol}' is not allowed`;
    }
  }
  return null;
}

const FIND_ALLOWED_TOKENS = new Set([
  '-E',
  '-O3',
  '-empty',
  '-executable',
  '-readable',
  '-writable',
  '-prune',
  '-print0',
  '(',
  ')',
  '-o',
]);

const FIND_ALLOWED_TOKENS_WITH_VALUES = new Set([
  '-maxdepth',
  '-mindepth',
  '-type',
  '-name',
  '-iname',
  '-path',
  '-regex',
  '-regextype',
  '-size',
  '-mtime',
  '-mmin',
  '-atime',
  '-amin',
  '-perm',
]);

interface CommandValidationResult {
  isValid: boolean;
  error?: string;
}

export function validateCommand(
  command: string,
  args: string[]
): CommandValidationResult {
  if (!Array.isArray(args)) {
    return {
      isValid: false,
      error: 'Arguments must be an array',
    };
  }

  const normalized = normalizeCommandName(command);

  const extraCmds = securityRegistry.extraAllowedCommands;
  const isBuiltinAllowed = ALLOWED_COMMANDS.includes(
    normalized as (typeof ALLOWED_COMMANDS)[number]
  );
  const isExtraAllowed = extraCmds.includes(normalized);
  if (!isBuiltinAllowed && !isExtraAllowed) {
    const all = [...ALLOWED_COMMANDS, ...extraCmds];
    return {
      isValid: false,
      error: `Command '${command}' is not allowed. Allowed commands: ${all.join(', ')}`,
    };
  }

  return validateCommandArgs(normalized, args);
}

function validateCommandArgs(
  command: string,
  args: string[]
): CommandValidationResult {
  if (command === 'rg') {
    const disallowedFlag = findDisallowedRgFlag(args);
    if (disallowedFlag) {
      return {
        isValid: false,
        error: `rg option '${disallowedFlag}' is not allowed.`,
      };
    }
  } else if (command === 'git') {
    const gitError = validateGitArgs(args);
    if (gitError) {
      return { isValid: false, error: gitError };
    }
  } else if (command === 'find') {
    const invalidFindArg = findInvalidFindArg(args);
    if (invalidFindArg) {
      return {
        isValid: false,
        error: `find operator '${invalidFindArg}' is not allowed.`,
      };
    }
  }

  const patternPositions = getPatternArgPositions(command, args);

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    const isPattern = patternPositions.has(i);

    const dangerousPatterns = isPattern
      ? PATTERN_DANGEROUS_PATTERNS
      : DANGEROUS_PATTERNS;

    for (const dangerousPattern of dangerousPatterns) {
      if (dangerousPattern.test(arg)) {
        const argType = isPattern ? 'search pattern' : 'argument';
        return {
          isValid: false,
          error: `Dangerous pattern detected in ${argType}: '${arg}'. This may be a command injection attempt.`,
        };
      }
    }
  }

  return { isValid: true };
}

const RG_GLOB_FLAGS = new Set([
  '-g',
  '--glob',
  '--include',
  '--exclude',
  '--exclude-dir',
]);

function getRgPatternPositions(args: string[]): Set<number> {
  const positions = new Set<number>();
  let foundPattern = false;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    if (arg === '--') {
      if (i + 1 < args.length) positions.add(i + 1);
      break;
    }
    if (arg.startsWith('-')) {
      if (RG_GLOB_FLAGS.has(arg)) {
        i++;
        positions.add(i);
      } else if (RG_ALLOWED_FLAGS_WITH_VALUES.has(arg)) {
        i++;
      }
      continue;
    }
    if (!foundPattern) {
      positions.add(i);
      foundPattern = true;
    }
  }
  return positions;
}

function getGrepPatternPositions(args: string[]): Set<number> {
  const positions = new Set<number>();
  let foundPattern = false;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    if (arg === '--') {
      if (i + 1 < args.length) positions.add(i + 1);
      break;
    }
    if (arg.startsWith('--include=') || arg.startsWith('--exclude=')) {
      positions.add(i);
    } else if (!arg.startsWith('-') && !foundPattern) {
      positions.add(i);
      foundPattern = true;
    }
  }
  return positions;
}

const FIND_PATTERN_ARGS = new Set([
  '-name',
  '-iname',
  '-path',
  '-regex',
  '-size',
  '-perm',
]);

function getFindPatternPositions(args: string[]): Set<number> {
  const positions = new Set<number>();
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    const prevArg = i > 0 ? args[i - 1]! : '';
    if (FIND_PATTERN_ARGS.has(prevArg)) positions.add(i);
    if (arg === '(' || arg === ')' || arg === '-o') positions.add(i);
  }
  return positions;
}

function getPatternArgPositions(command: string, args: string[]): Set<number> {
  switch (command) {
    case 'rg':
      return getRgPatternPositions(args);
    case 'grep':
      return getGrepPatternPositions(args);
    case 'find':
      return getFindPatternPositions(args);
    default:
      return new Set();
  }
}

function findDisallowedRgFlag(args: string[]): string | null {
  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    if (arg === '--') {
      return null;
    }

    if (!arg.startsWith('-')) {
      return null;
    }

    if (arg.startsWith('--pre') || arg.startsWith('--pre-glob')) {
      return arg;
    }

    if (RG_ALLOWED_FLAGS_WITH_VALUES.has(arg)) {
      i++;
      continue;
    }

    if (RG_ALLOWED_FLAGS.has(arg)) {
      continue;
    }

    if (/^-[a-zA-Z]{2,}$/.test(arg)) {
      const chars = arg.slice(1);
      const allAllowed = [...chars].every(ch => RG_ALLOWED_SHORT_FLAGS.has(ch));
      if (allAllowed) {
        continue;
      }
      return arg;
    }

    return arg;
  }

  return null;
}

function validateGitArgs(args: string[]): string | null {
  if (args.length === 0) {
    return 'git command requires a subcommand';
  }

  let subcommandIndex = 0;
  while (subcommandIndex < args.length) {
    const arg = args[subcommandIndex]!;
    if (arg === '-c') {
      const keyValue = args[subcommandIndex + 1];
      if (keyValue !== undefined) {
        const configError = validateGitConfigKeyValue(keyValue);
        if (configError) return configError;
      }
      subcommandIndex += 2;
      continue;
    }
    if (arg === '-C') {
      subcommandIndex += 2;
      continue;
    }
    break;
  }

  if (subcommandIndex >= args.length) {
    return 'git command requires a subcommand';
  }

  const subcommand = args[subcommandIndex]!;
  if (!GIT_ALLOWED_SUBCOMMANDS.has(subcommand)) {
    return `git subcommand '${subcommand}' is not allowed. Allowed: ${[...GIT_ALLOWED_SUBCOMMANDS].join(', ')}`;
  }

  if (subcommand === 'clone') {
    return validateGitCloneArgs(args, subcommandIndex);
  } else if (subcommand === 'sparse-checkout') {
    return validateGitSparseCheckoutArgs(args, subcommandIndex);
  }

  return null;
}

const GIT_CLONE_FLAGS_WITH_VALUES = new Set([
  '--depth',
  '--branch',
  '--filter',
]);

function processCloneFlag(
  args: string[],
  i: number
): { skip: number; error?: string } {
  const arg = args[i]!;

  if (!GIT_CLONE_ALLOWED_FLAGS.has(arg)) {
    return { skip: 0, error: `git clone flag '${arg}' is not allowed` };
  }

  if (GIT_CLONE_FLAGS_WITH_VALUES.has(arg)) return { skip: 1 };

  if (arg === '-c' && i + 1 < args.length) {
    const configError = validateGitConfigKeyValue(args[i + 1]!);
    if (configError) return { skip: 1, error: configError };
    return { skip: 1 };
  }

  return { skip: 0 };
}

function validateGitCloneArgs(
  args: string[],
  subcommandIndex: number
): string | null {
  let pastEndOfFlags = false;
  let urlFound = false;

  for (let i = subcommandIndex + 1; i < args.length; i++) {
    const arg = args[i]!;

    if (arg === '--') {
      pastEndOfFlags = true;
      continue;
    }

    if (pastEndOfFlags || !arg.startsWith('-')) {
      if (!urlFound) {
        const urlError = validateGitCloneUrl(arg);
        if (urlError) return urlError;
        urlFound = true;
      }
      continue;
    }

    const { skip, error } = processCloneFlag(args, i);
    if (error) return error;
    i += skip;
  }

  return null;
}

function validateGitSubcommandFlags(
  args: string[],
  startIndex: number,
  allowedFlags: Set<string>,
  label: string
): string | null {
  for (let i = startIndex; i < args.length; i++) {
    const arg = args[i]!;
    if (!arg.startsWith('-')) continue;
    if (!allowedFlags.has(arg)) {
      return `git ${label} flag '${arg}' is not allowed`;
    }
  }
  return null;
}

function validateGitSparseCheckoutArgs(
  args: string[],
  subcommandIndex: number
): string | null {
  const actionIndex = subcommandIndex + 1;
  if (actionIndex >= args.length) {
    return 'git sparse-checkout requires an action (init, set, add, list, disable)';
  }
  const action = args[actionIndex]!;
  if (!GIT_SPARSE_CHECKOUT_ALLOWED_ACTIONS.has(action)) {
    return `git sparse-checkout action '${action}' is not allowed`;
  }
  return validateGitSubcommandFlags(
    args,
    actionIndex + 1,
    GIT_SPARSE_CHECKOUT_ALLOWED_FLAGS,
    'sparse-checkout'
  );
}

function findInvalidFindArg(args: string[]): string | null {
  let afterPathArgs = false;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    if (arg === '--') {
      continue;
    }

    if (!afterPathArgs && !arg.startsWith('-') && arg !== '(' && arg !== ')') {
      continue;
    }
    afterPathArgs = true;

    if (FIND_DISALLOWED_OPERATORS.has(arg)) {
      return arg;
    }

    if (FIND_ALLOWED_TOKENS_WITH_VALUES.has(arg)) {
      i++;
      continue;
    }

    if (FIND_ALLOWED_TOKENS.has(arg)) {
      continue;
    }

    if (!arg.startsWith('-')) {
      continue;
    }

    return arg;
  }

  return null;
}
