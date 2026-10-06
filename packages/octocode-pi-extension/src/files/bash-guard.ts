import type { ToolCallEvent, ToolCallEventResult } from '@earendil-works/pi-coding-agent';

/** Targets whose recursive removal wipes the system or the user's home. */
const ROOT_TARGETS = new Set(['/', '/*', '/.', '~', '~/*', '$HOME', '${HOME}', '$HOME/*', '${HOME}/*']);
const WRAPPERS = new Set(['sudo', 'doas', 'command', 'exec', 'nohup', 'time', 'env', 'xargs']);
/** Wrapper options that take the next word as their value (`sudo -u root rm …`: `root` is not the command). */
const OPTION_ARGS: Record<string, ReadonlySet<string>> = {
  sudo: new Set(['-u', '-g', '-h', '-p', '-C', '-D', '-R', '-r', '-T', '-t', '-U', '--user', '--group', '--host', '--prompt', '--close-from', '--chdir', '--chroot', '--role', '--type', '--command-timeout', '--other-user']),
  doas: new Set(['-u', '-C']),
  env: new Set(['-u', '--unset', '-C', '--chdir']),
};
const SYSTEM = /^(?:mkfs(?:\.[a-z0-9]+)?|shutdown|reboot|halt|poweroff)$/i;
/** dd writing to a device; the pseudo devices used to discard or print output are fine. */
const DD_DEVICE = /^of=\/dev\/(?!(?:null|zero|stdout|stderr|tty)$|fd\/)/i;
const FORK_BOMB = /:\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;?\s*:/;
/** Stands in for whitespace and shell operators inside quotes, so a quoted string stays one word and splits nothing. */
const HIDDEN = '\u0001';
const OPERATOR = /[\s;&|()`<>]/g;
const HEREDOC = /^<<(-?)[ \t]*(['"]?)([\w.-]+)\2/;

const hide = (text: string): string => text.replace(OPERATOR, HIDDEN);
const base = (token: string): string => token.slice(token.lastIndexOf('/') + 1);

/**
 * The command with quotes removed (their content kept as one word, with operators neutralised) and heredoc bodies
 * dropped: what is left is what the shell splits into commands.
 */
function mask(command: string): string {
  let out = '';
  const heredocs: Array<{ tag: string; tabs: boolean }> = [];
  let index = 0;
  while (index < command.length) {
    const char = command[index]!;
    if (char === '\n' && heredocs.length > 0) {
      out += '\n';
      index++;
      // Skip body lines up to each pending delimiter, in order.
      while (heredocs.length > 0 && index < command.length) {
        const end = command.indexOf('\n', index);
        const stop = end < 0 ? command.length : end;
        const line = command.slice(index, stop);
        index = stop + 1;
        const { tag, tabs } = heredocs[0]!;
        if ((tabs ? line.replace(/^\t+/, '') : line) === tag) heredocs.shift();
      }
      continue;
    }
    if (char === '\\') {
      const next = command[index + 1] ?? '';
      out += next === '\n' ? ' ' : hide(next);
      index += 2;
      continue;
    }
    if (char === "'") {
      const end = command.indexOf("'", index + 1);
      const stop = end < 0 ? command.length : end;
      out += hide(command.slice(index + 1, stop));
      index = stop + 1;
      continue;
    }
    if (char === '"') {
      let body = '';
      const substitutions: string[] = [];
      let cursor = index + 1;
      while (cursor < command.length && command[cursor] !== '"') {
        if (command[cursor] === '\\' && cursor + 1 < command.length) {
          body += command[cursor + 1];
          cursor += 2;
        } else if (command.startsWith('$(', cursor) || command[cursor] === '`') {
          // `$(…)` and backticks run even inside double quotes: check their body as separate commands.
          const end = substitutionEnd(command, cursor);
          substitutions.push(command.slice(cursor + (command[cursor] === '`' ? 1 : 2), end));
          cursor = end + 1;
        } else body += command[cursor++];
      }
      out += hide(body) + substitutions.map((inner) => `;${mask(inner)};`).join('');
      index = cursor + 1;
      continue;
    }
    const heredoc = char === '<' && command[index - 1] !== '<' ? HEREDOC.exec(command.slice(index)) : null;
    if (heredoc && command[index + 2] !== '<') {
      heredocs.push({ tag: heredoc[3]!, tabs: heredoc[1] === '-' });
      out += ' ';
      index += heredoc[0].length;
      continue;
    }
    out += char;
    index++;
  }
  return out;
}

/** Index of the character closing the `$(…)` or backtick substitution that starts at `start`. */
function substitutionEnd(command: string, start: number): number {
  if (command[start] === '`') {
    const end = command.indexOf('`', start + 1);
    return end < 0 ? command.length : end;
  }
  let depth = 0;
  for (let at = start + 1; at < command.length; at++) {
    if (command[at] === '\\') at++;
    else if (command[at] === '(') depth++;
    else if (command[at] === ')' && --depth === 0) return at;
  }
  return command.length;
}

function words(segment: string): string[] {
  const tokens = segment.trim().split(/\s+/).filter(Boolean).map((token) => token.replaceAll(HIDDEN, ' '));
  // Skip wrappers, their options (and option values) and VAR=value prefixes to reach the command itself.
  let wrapper: string | undefined;
  while (tokens.length > 0) {
    const token = tokens[0]!;
    if (WRAPPERS.has(base(token))) {
      wrapper = base(token);
      tokens.shift();
    } else if (/^\w+=/.test(token)) {
      tokens.shift();
    } else if (token.startsWith('-') && tokens.length > 1) {
      tokens.shift();
      if (wrapper && OPTION_ARGS[wrapper]?.has(token)) tokens.shift();
    } else break;
  }
  return tokens;
}

function wipesRoot(tokens: string[]): boolean {
  if (base(tokens[0] ?? '') !== 'rm') return false;
  const flags = tokens.slice(1).filter((token) => token.startsWith('-'));
  if (flags.includes('--no-preserve-root')) return true;
  const recursive = flags.some((flag) => flag === '--recursive' || /^-[a-z]*r/i.test(flag));
  const targets = tokens.slice(1).filter((token) => !token.startsWith('-')).map((token) => (token.length > 1 ? token.replace(/\/+$/, '') || '/' : token));
  return recursive && targets.some((target) => ROOT_TARGETS.has(target));
}

/** Why a shell command would be catastrophic (wipe `/` or home, format a disk, power off, fork-bomb), else undefined. */
export function catastrophicCommand(command: string): string | undefined {
  const masked = mask(command);
  if (FORK_BOMB.test(masked)) return 'a fork bomb';
  for (const segment of masked.split(/[;&|\n()`]|\$\(/)) {
    const tokens = words(segment);
    const name = base(tokens[0] ?? '');
    if (wipesRoot(tokens)) return `recursive removal of ${tokens.slice(1).filter((token) => !token.startsWith('-')).join(' ') || '/'}`;
    if (SYSTEM.test(name)) return `\`${name}\``;
    if (name === 'dd' && tokens.slice(1).some((token) => DD_DEVICE.test(token))) return 'dd writing to a device';
  }
  return undefined;
}

/** Tool-call gate: refuses catastrophic bash commands (wiping `/` or home, formatting a disk, powering off, a fork bomb). */
export async function bashSafetyGate(event: ToolCallEvent): Promise<ToolCallEventResult | undefined> {
  if (event.toolName !== 'bash') return undefined;
  const command = typeof event.input['command'] === 'string' ? event.input['command'] : '';
  const danger = catastrophicCommand(command);
  return danger ? { block: true, reason: `Refused: ${danger} could destroy the system or its data. If it is truly needed, ask the user to run it themselves.` } : undefined;
}
