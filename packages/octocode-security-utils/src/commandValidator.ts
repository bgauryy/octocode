/**
 * Command validation for security - prevents command injection attacks
 */

import {
  ALLOWED_COMMANDS,
  DANGEROUS_PATTERNS,
  PATTERN_DANGEROUS_PATTERNS,
} from './securityConstants.js';
import { securityRegistry } from './registry.js';

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

/** Single-character flags extracted from RG_ALLOWED_FLAGS for bundle validation */
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

/**
 * Git: only allow specific subcommands and flags for security.
 * - clone: shallow-clone repositories (githubCloneRepo tool)
 * - sparse-checkout: partial tree fetching for specific paths
 */
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
  '--', // end-of-flags separator (security: prevents positional args being parsed as flags)
]);

/** Allowed sub-subcommands for `git sparse-checkout` */
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
  '--', // end-of-flags separator (security: prevents path args being parsed as flags)
]);

/**
 * Allowlist of git config keys permitted via the -c flag.
 * Only keys needed for legitimate authentication/transport configuration are allowed.
 * ALL others are blocked — including those that enable arbitrary code execution:
 *   core.sshCommand, core.hooksPath, credential.helper, core.gitProxy, http.proxy,
 *   protocol.allow, etc.
 */
const GIT_SAFE_CONFIG_KEYS = new Set([
  'advice.detachedHead', // suppresses detached-HEAD warning message — display only
  'core.autocrlf',
  'core.sparseCheckout',
  'http.extraHeader',
  'http.followRedirects',
  'http.userAgent',
  'http.version',
]);

/**
 * Git clone URL protocols that are explicitly blocked for security.
 * - file://  → local filesystem access (can read /etc/passwd etc.)
 * - git://   → unauthenticated, no TLS, susceptible to MITM
 * - http://  → unencrypted transmission of credentials
 * Allowed: https://, git@ (SSH), ssh://
 */
const GIT_BLOCKED_URL_PROTOCOLS = ['file://', 'git://', 'http://'] as const;

/**
 * Validate a git -c key=value argument against the safe config key allowlist.
 * Returns an error string if the key is not in the allowlist, null if safe.
 */
function validateGitConfigKeyValue(keyValue: string): string | null {
  const eqIndex = keyValue.indexOf('=');
  const key = eqIndex >= 0 ? keyValue.substring(0, eqIndex) : keyValue;
  if (!GIT_SAFE_CONFIG_KEYS.has(key)) {
    return `git config key '${key}' is not allowed via -c`;
  }
  return null;
}

/**
 * Validate a git clone URL against blocked protocols.
 * Returns an error string if the URL uses a blocked protocol, null if safe.
 */
function validateGitCloneUrl(url: string): string | null {
  for (const protocol of GIT_BLOCKED_URL_PROTOCOLS) {
    if (url.startsWith(protocol)) {
      return `git clone URL protocol '${protocol}' is not allowed`;
    }
  }
  return null;
}

const FIND_ALLOWED_TOKENS = new Set([
  '-E', // macOS BSD find: enable extended regex (must appear before path)
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

/**
 * Result of command validation
 */
interface CommandValidationResult {
  isValid: boolean;
  error?: string;
}

/**
 * Validates that a command is allowed and safe to execute.
 * Uses command-aware validation to allow legitimate patterns.
 *
 * @example
 * ```ts
 * validateCommand('rg', ['--json', 'pattern', './src']);
 * // → { isValid: true }
 * validateCommand('rm', ['-rf', '/']);
 * // → { isValid: false, error: "Command 'rm' is not allowed. ..." }
 * ```
 */
export function validateCommand(
  command: string,
  args: string[]
): CommandValidationResult {
  // Guard: args must be an array to prevent TypeError on .length / iteration
  if (!Array.isArray(args)) {
    return {
      isValid: false,
      error: 'Arguments must be an array',
    };
  }

  const extraCmds = securityRegistry.extraAllowedCommands;
  const isBuiltinAllowed = ALLOWED_COMMANDS.includes(
    command as (typeof ALLOWED_COMMANDS)[number]
  );
  const isExtraAllowed = extraCmds.includes(command);
  if (!isBuiltinAllowed && !isExtraAllowed) {
    const all = [...ALLOWED_COMMANDS, ...extraCmds];
    return {
      isValid: false,
      error: `Command '${command}' is not allowed. Allowed commands: ${all.join(', ')}`,
    };
  }

  // Command-aware validation
  return validateCommandArgs(command, args);
}

/**
 * Validates arguments based on command context
 * Uses position-aware validation - certain args are search patterns, others are paths
 *
 * Pattern arguments (regex, globs) get more permissive validation that allows
 * legitimate regex metacharacters but still blocks shell injection vectors.
 */
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

  // Define which argument positions contain patterns (not paths/filenames)
  // Patterns can safely contain |, (), etc. as they're regex/search patterns
  const patternPositions = getPatternArgPositions(command, args);

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]!;
    const isPattern = patternPositions.has(i);

    // Use appropriate validation set based on argument type
    // Pattern args get more permissive checks but still block shell injection
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

    // Allow short flag bundles like -rnH if every character is an allowed single-char flag.
    // This prevents passing dangerous flags (e.g. -x for --pre) in a bundle.
    if (/^-[a-zA-Z]{2,}$/.test(arg)) {
      const chars = arg.slice(1); // remove leading '-'
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

/**
 * Validate git command arguments.
 * Only allows specific subcommands (clone, sparse-checkout) with safe flags.
 */
function validateGitArgs(args: string[]): string | null {
  if (args.length === 0) {
    return 'git command requires a subcommand';
  }

  // Find the subcommand, skipping global options before it:
  //   -c key=value  → git config override (key validated against allowlist)
  //   -C path       → change directory before running
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

/**
 * Process a single clone flag and return the number of extra args it consumes,
 * or an error string if the flag is disallowed or its value is invalid.
 */
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

/**
 * Validate arguments for `git clone`.
 * Validates flags against the allowlist, config keys against GIT_SAFE_CONFIG_KEYS,
 * and the clone URL against GIT_BLOCKED_URL_PROTOCOLS.
 */
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

/**
 * Validate flags in a git subcommand arg list against an allowlist.
 * Non-flag positional arguments are allowed through.
 */
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

/**
 * Validate arguments for `git sparse-checkout`.
 * Validates the action and any flags.
 */
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

    // find grammar is effectively: find <path...> <expr...>
    // First non-flag token is treated as path; expression starts at first flag.
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

    // Non-flag values are acceptable expression operands (e.g., path/pattern values)
    if (!arg.startsWith('-')) {
      continue;
    }

    return arg;
  }

  return null;
}
