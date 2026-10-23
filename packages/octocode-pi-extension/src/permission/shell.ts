import os from 'node:os';
import path from 'node:path';
import { base, BODY, hide, mask, SEPARATOR, SHELLS, shellScript, stdinOf, unhide, words } from './shell-parse.js';

/** Programs whose operands are the paths they remove. */
const REMOVERS = new Set(['rm', 'rmdir', 'unlink', 'shred']);
const SYSTEM = /^(?:mkfs(?:\.[a-z0-9]+)?|newfs(?:_[a-z0-9]+)?|shutdown|reboot|halt|poweroff)$/i;
/** `diskutil` verbs that erase a disk or volume. */
const DISKUTIL_ERASE = /^(?:erase\w*|zeroDisk|randomDisk|secureErase)$/i;
/** dd writing to a device; the pseudo devices used to discard or print output are fine. */
const DD_DEVICE = /^of=\/dev\/(?!(?:null|zero|stdout|stderr|tty)$|fd\/)/i;
const FORK_BOMB = /:\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;?\s*:/;

/** Where a command runs: the home folder, and the shell's folder when known (`cd` moves it). */
interface Place {
  home: string;
  cwd: string | undefined;
}

/** `path` with the platform separator written as `/`. */
const posix = (file: string): string => file.split(path.sep).join('/');

/** `target` with a leading `~`, `$HOME` or `${HOME}` replaced by `home`. */
export const expandHome = (target: string, home: string): string => target.replace(/^(?:~|\$HOME|\$\{HOME\})(?=\/|$)/, home);

/**
 * Where a target lands with `~`, `$HOME` and `${HOME}` expanded and `.`/`..` resolved against the shell's folder; a
 * trailing `/*` glob counts as its folder. Undefined when it cannot be placed: relative with no folder known, or built
 * from another variable.
 */
function locate(target: string, { home, cwd }: Place): string | undefined {
  const expanded = expandHome(unhide(target), home);
  if (/\$/.test(expanded)) return undefined;
  const absolute = expanded.startsWith('/') ? expanded : cwd?.startsWith('/') ? path.posix.resolve(cwd, expanded) : undefined;
  if (absolute === undefined) return undefined;
  const normalized = path.posix.normalize(absolute).replace(/\/\.?\*$/, '') || '/';
  return normalized.length > 1 ? normalized.replace(/\/+$/, '') || '/' : normalized;
}

/** Folders whose files a command should not remove or re-permission: the system's own, and the SSH keys. */
const SYSTEM_DIRS = ['/usr', '/etc', '/bin', '/sbin', '/lib', '/lib64', '/boot', '/System'];

const isUnder = (where: string, dir: string): boolean => where === dir || where.startsWith(`${dir}/`);

/** The system folder (or `~/.ssh`) `where` is in, else undefined. */
const systemDir = (where: string, home: string): string | undefined => [...SYSTEM_DIRS, `${home}/.ssh`].find((dir) => isUnder(where, dir));

/** Whether acting on `where` recursively would reach `/`, the home folder or a folder holding it. */
const holdsHome = (where: string, home: string): boolean => where === '/' || home === where || home.startsWith(`${where}/`);

/** The protected place `target` names (`/`, home or above it, a system folder, `~/.ssh`), else undefined. */
function protectedPlace(target: string, place: Place): string | undefined {
  const where = locate(target, place);
  if (where === undefined) return undefined;
  return holdsHome(where, place.home) ? where : systemDir(where, place.home) !== undefined ? where : undefined;
}

const operandsOf = (tokens: string[]): string[] => tokens.slice(1).filter((token) => !token.startsWith('-'));

/** Why this `rm`-like command is destructive: recursive removal of `/`, home or a folder holding it, or any removal in a system folder. */
function removesProtected(tokens: string[], place: Place): string | undefined {
  const name = base(tokens[0] ?? '');
  if (!REMOVERS.has(name)) return undefined;
  const flags = tokens.slice(1).filter((token) => token.startsWith('-'));
  const targets = operandsOf(tokens);
  if (name === 'rm' && flags.includes('--no-preserve-root')) return `recursive removal of ${targets.map(unhide).join(' ') || '/'}`;
  const recursive = name === 'rm' && flags.some((flag) => flag === '--recursive' || /^-[a-z]*r/i.test(flag));
  for (const target of targets) {
    const where = locate(target, place);
    if (where === undefined) continue;
    if (recursive && holdsHome(where, place.home)) return `recursive removal of ${unhide(target)}${where === unhide(target) ? '' : ` (${where})`}`;
    const system = systemDir(where, place.home);
    if (system) return `removal under ${system}`;
  }
  return undefined;
}

/** `chmod -R`, `chown -R` or `chgrp -R` on `/`, home or a folder holding it, a system folder or `~/.ssh`. */
function changesTree(tokens: string[], place: Place): string | undefined {
  const name = base(tokens[0] ?? '');
  if (name !== 'chmod' && name !== 'chown' && name !== 'chgrp') return undefined;
  if (!tokens.slice(1).some((token) => token === '--recursive' || /^-[a-zA-Z]*R/.test(token))) return undefined;
  const target = operandsOf(tokens).find((operand) => protectedPlace(operand, place) !== undefined);
  return target === undefined ? undefined : `\`${name} -R\` on ${unhide(target)}`;
}

/** `find <protected folder> … -delete` (or `-exec rm`): removes everything under `/`, home or a system folder. */
function findDeletesRoot(tokens: string[], place: Place): boolean {
  if (base(tokens[0] ?? '') !== 'find') return false;
  const rest = tokens.slice(1);
  while (/^-[HLP]$/.test(rest[0] ?? '')) rest.shift();
  const end = rest.findIndex((token) => /^[-(!]/.test(token));
  const roots = end < 0 ? rest : rest.slice(0, end);
  const deletes = rest.some((token, at) => token === '-delete' || (/^-exec(?:dir)?$/.test(token) && base(rest[at + 1] ?? '') === 'rm'));
  return deletes && roots.some((root) => protectedPlace(root, place) !== undefined);
}

/** Block devices a stray write destroys (`/dev/sda`, `/dev/nvme0n1`, `/dev/disk2`, `/dev/rdisk2`, …). */
const BLOCK_DEVICE = /^\/dev\/(?:sd[a-z]|hd[a-z]|vd[a-z]|xvd[a-z]|nvme\d|mmcblk\d|r?disk\d)/i;

/** The block device a simple command writes to (a redirect, `tee`, `shred`, `truncate`, `wipefs`, `blkdiscard`, or the last operand of `cp`/`mv`), else undefined. */
function writesDevice(tokens: string[]): string | undefined {
  const name = base(tokens[0] ?? '');
  const words = tokens.map(unhide);
  for (const [at, word] of words.entries()) {
    const redirect = /^(?:\d*|&)>>?\|?(.*)$/.exec(word);
    if (!redirect) continue;
    const target = redirect[1] || words[at + 1] || '';
    if (BLOCK_DEVICE.test(target)) return target;
  }
  const targets = operandsOf(words);
  const written = new Set(['tee', 'shred', 'truncate', 'wipefs', 'blkdiscard']).has(name) ? targets : ['cp', 'mv'].includes(name) ? targets.slice(-1) : [];
  return written.find((target) => BLOCK_DEVICE.test(target));
}


function emitted(tokens: string[], bodies: string[]): string[] {
  const name = base(tokens[0] ?? '');
  if (name === 'echo' || name === 'printf') return [tokens.slice(1).filter((token) => !/^-[a-z]+$/i.test(token)).join(' ')];
  return name === 'cat' ? stdinOf(tokens, bodies) : [];
}

/**
 * The shell's folder after `cd <target>` (or `pushd`), `home` for a bare `cd`; undefined when it cannot be told (`cd -`,
 * `cd $DIR`).
 */
function afterCd(tokens: string[], place: Place): string | undefined {
  const target = tokens.slice(1).find((token) => !/^-[LPe@]+$/.test(token));
  return target === undefined ? place.home : target === '-' ? undefined : locate(target, place);
}

/**
 * Why a shell command would be destructive (wipe `/` or home, `rm` in a system folder or `~/.ssh`, `chmod -R` on a
 * protected root, write to a disk device, format a disk, power off, fork-bomb), else undefined. Relative targets resolve
 * against `cwd`, which `cd` moves along the command line. It flags common destructive commands by reading the command's
 * words; it is not a security boundary (quoting and variable tricks get past it).
 */
export function catastrophicCommand(command: string, cwd?: string): string | undefined {
  return destructiveIn(command, { home: posix(os.homedir()), cwd: cwd === undefined ? undefined : posix(cwd) });
}

function destructiveIn(command: string, start: Place): string | undefined {
  const bodies: string[] = [];
  const masked = mask(command, bodies);
  if (FORK_BOMB.test(masked)) return 'a fork bomb';
  const parts = masked.split(SEPARATOR);
  let place = start;
  let piped: string[] = [];
  for (let at = 0; at < parts.length; at += 2) {
    const tokens = words(parts[at]!);
    const fromPipe = parts[at - 1] === '|' ? piped : [];
    piped = emitted(tokens, bodies).map(unhide);
    const name = base(tokens[0] ?? '');
    const removed = removesProtected(tokens, place);
    if (removed) return removed;
    if (findDeletesRoot(tokens, place)) return `\`find\` deleting everything under ${unhide(tokens[1] ?? '/')}`;
    const changed = changesTree(tokens, place);
    if (changed) return changed;
    if (SYSTEM.test(name)) return `\`${name}\``;
    if (name === 'diskutil' && DISKUTIL_ERASE.test(tokens[1] ?? '')) return `\`diskutil ${tokens[1]}\``;
    if (name === 'dd' && tokens.slice(1).some((token) => DD_DEVICE.test(unhide(token)))) return 'dd writing to a device';
    const device = writesDevice(tokens);
    if (device) return `a write to ${device}`;
    if (name === 'cd' || name === 'pushd') place = { ...place, cwd: afterCd(tokens, place) };
    const scripts: string[] = [];
    if (name === 'eval') scripts.push(tokens.slice(1).map(unhide).join(' '));
    if (SHELLS.has(name)) {
      const shell = shellScript(tokens);
      if (shell.script !== undefined) scripts.push(unhide(shell.script));
      if (shell.stdin) scripts.push(...stdinOf(tokens, bodies).map(unhide), ...fromPipe);
    }
    for (const script of scripts) {
      const danger = destructiveIn(script, place);
      if (danger) return danger;
    }
  }
  return undefined;
}

/**
 * The base program name of every simple command `command` runs (past wrappers and `VAR=value`), including the scripts
 * of `sh -c`, `eval` and `$(…)`, each once in order of appearance.
 */
export function commandNames(command: string): string[] {
  const bodies: string[] = [];
  const parts = mask(command, bodies).split(SEPARATOR);
  const names: string[] = [];
  for (let at = 0; at < parts.length; at += 2) {
    const tokens = words(parts[at]!);
    const name = base(unhide(tokens[0] ?? ''));
    if (!name || BODY.test(name)) continue;
    names.push(name);
    if (name === 'eval') names.push(...commandNames(tokens.slice(1).map(unhide).join(' ')));
    const script = SHELLS.has(name) ? shellScript(tokens).script : undefined;
    if (script !== undefined) names.push(...commandNames(unhide(script)));
  }
  return [...new Set(names)];
}

/** The paths `rm`, `rmdir`, `unlink` and `shred` name in `command` (past wrappers and `sh -c` scripts), as written, each once. */
export function removalTargets(command: string): string[] {
  const bodies: string[] = [];
  const found: string[] = [];
  for (const [at, part] of mask(command, bodies).split(SEPARATOR).entries()) {
    if (at % 2 === 1) continue;
    const tokens = words(part).map(unhide);
    const name = base(tokens[0] ?? '');
    if (REMOVERS.has(name)) {
      const rest = tokens.slice(1);
      const end = rest.indexOf('--');
      found.push(...rest.filter((token, index) => (end >= 0 ? index > end : !token.startsWith('-'))));
    }
    const script = SHELLS.has(name) ? shellScript(tokens.map(hide)).script : undefined;
    if (script !== undefined) found.push(...removalTargets(unhide(script)));
  }
  return [...new Set(found)];
}

/** `command` trimmed when it is one simple command (no `;`, `&&`, pipes, substitutions or heredocs), else undefined. */
export function simpleCommand(command: string): string | undefined {
  const trimmed = command.trim();
  const bodies: string[] = [];
  const parts = mask(trimmed, bodies).split(SEPARATOR);
  return trimmed !== '' && parts.length === 1 && bodies.length === 0 ? trimmed : undefined;
}

/** Programs that only read and print: running one again gives the same view of the same state, and writes nothing. */
const READERS = new Set([
  'cat', 'head', 'tail', 'ls', 'wc', 'grep', 'egrep', 'fgrep', 'rg', 'nl', 'stat', 'file', 'du', 'df', 'pwd', 'echo', 'printf', 'which', 'diff', 'cmp', 'uniq', 'cut', 'tr',
  'basename', 'dirname', 'realpath', 'readlink', 'tree', 'jq', 'column', 'sort', 'find', 'md5sum', 'shasum', 'sha256sum', 'date', 'uname', 'whoami', 'id', 'cd', 'true', 'false', 'test', '[',
]);
/** Whether an option token makes a reader run another program or write a file. */
type OptionTest = (option: string) => boolean;
/** A long option by any unambiguous-looking abbreviation (`--out`, `--output=x`): getopt and git accept them, so an exact match would miss them. */
const long = (...names: string[]): OptionTest => (option) => {
  if (!option.startsWith('--')) return false;
  const flag = option.slice(2).split('=')[0]!;
  return flag !== '' && names.some((name) => name.startsWith(flag));
};
/** A short option anywhere in a cluster (`-ro`), or one with its value attached (`-ofile`): either way the letter is in the token. */
const short = (letters: string): OptionTest => {
  const cluster = new RegExp(`^-[a-zA-Z0-9]*[${letters}]`);
  return (option) => cluster.test(option);
};
const exact = (...options: string[]): OptionTest => (option) => options.includes(option);
const anyOf = (...tests: OptionTest[]): OptionTest => (option) => tests.some((test) => test(option));

/** Options that make a reader run another program or write a file. */
const WRITING_OPTIONS: Record<string, OptionTest> = {
  find: exact('-exec', '-execdir', '-ok', '-okdir', '-delete', '-fprint', '-fprint0', '-fprintf', '-fls'),
  sort: anyOf(short('o'), long('output', 'compress-program')),
  rg: anyOf(long('pre', 'hostname-bin')),
  tree: short('o'),
  file: anyOf(short('C'), long('compile')),
  date: anyOf(short('s'), long('set')),
  // `-O<pager>` runs a program on the matches, `--ext-diff` and `--textconv` run the configured filters, `--output` and `-o` write a file.
  git: anyOf(exact('-o'), short('O'), long('output', 'ext-diff', 'textconv', 'open-files-in-pager')),
};
/** `uniq` writes its result to a second operand; its value-taking options (`-f 2`) are not operands. */
const UNIQ_VALUED = new Set(['-f', '-s', '-w', '--skip-fields', '--skip-chars', '--check-chars']);
const writesOperand = (options: string[]): boolean => {
  let operands = 0;
  for (let at = 0; at < options.length; at++) {
    const option = options[at]!;
    if (UNIQ_VALUED.has(option)) at++;
    else if (!option.startsWith('-') || option === '-') operands++;
  }
  return operands > 1;
};
/** `git` subcommands that only read the repository. */
const GIT_READERS = new Set(['status', 'log', 'diff', 'show', 'blame', 'ls-files', 'ls-tree', 'rev-parse', 'rev-list', 'describe', 'shortlog', 'grep', 'cat-file', 'show-ref']);
/** Redirections that write nowhere: to a file descriptor (`2>&1`), or to exactly `/dev/null` (not `/dev/nullx`, not `/dev/null/../x`). */
const HARMLESS_REDIRECT = /(?:&|\d*)>>?\s*(?:&\d+|\/dev\/null)(?=\s|$)/g;

/**
 * Whether every command `command` runs only reads: each simple command (in a pipeline, after `&&` or `;`, in a
 * substitution, or behind a wrapper such as `sudo` or `xargs`) is a program of `READERS` or a read-only `git`
 * subcommand, without an option that writes a file or runs another program (`WRITING_OPTIONS`, abbreviations
 * included), and nothing is redirected but to a file descriptor or `/dev/null`. A command the list does not know is
 * not read-only, and neither is a shell given a script (`sh -c`) or `uniq` given an output operand. A policy for what
 * may be left out of the context (its output can be had again), not a security check.
 */
export function readOnlyCommand(command: string): boolean {
  const bodies: string[] = [];
  const parts = mask(command, bodies).replace(HARMLESS_REDIRECT, ' ').split(SEPARATOR);
  let commands = 0;
  for (let at = 0; at < parts.length; at += 2) {
    const segment = parts[at]!;
    if (segment.includes('>')) return false;
    const tokens = words(segment).map(unhide);
    const name = tokens[0];
    if (name === undefined) continue;
    commands++;
    if (name.includes('/') || (!READERS.has(name) && name !== 'git')) return false;
    const options = tokens.slice(1);
    if (name === 'git' && !GIT_READERS.has(options[0] ?? '')) return false;
    if (options.some((option) => WRITING_OPTIONS[name]?.(option))) return false;
    if (name === 'uniq' && writesOperand(options)) return false;
  }
  return commands > 0;
}

/** The tools that run a shell command: Pi's `bash` and `powershell`. */
export const SHELL_TOOLS: ReadonlySet<string> = new Set(['bash', 'powershell']);
