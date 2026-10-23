/**
 * Reading a shell command line as the shell splits it: quotes, escapes, heredocs and substitutions are masked so that
 * what is left splits at `SEPARATOR` into simple commands, and `words` gives a simple command's words past wrappers.
 * The checks over those words are shell.ts.
 */

/** Commands that only run the rest of the line (after their own options and leading operands) as another command. */
const WRAPPERS = new Set(['sudo', 'doas', 'command', 'exec', 'nohup', 'time', 'env', 'xargs', 'timeout', 'nice', 'ionice', 'stdbuf', 'caffeinate', 'chrt', 'taskset']);
/** Wrapper options that take the next word as their value (`sudo -u root rm …`: `root` is not the command). */
const OPTION_ARGS: Record<string, ReadonlySet<string>> = {
  sudo: new Set(['-u', '-g', '-h', '-p', '-C', '-D', '-R', '-r', '-T', '-t', '-U', '--user', '--group', '--host', '--prompt', '--close-from', '--chdir', '--chroot', '--role', '--type', '--command-timeout', '--other-user']),
  doas: new Set(['-u', '-C']),
  env: new Set(['-u', '--unset', '-C', '--chdir']),
  xargs: new Set(['-I', '-J', '-L', '-n', '-P', '-s', '-d', '-E', '-a', '-R', '-S', '--max-args', '--max-lines', '--max-procs', '--max-chars', '--delimiter', '--eof', '--arg-file', '--replace', '--process-slot-var']),
  timeout: new Set(['-k', '-s', '--kill-after', '--signal']),
  nice: new Set(['-n', '--adjustment']),
  ionice: new Set(['-c', '-n', '-p', '-P', '-u', '--class', '--classdata', '--pid', '--pgid', '--uid']),
  stdbuf: new Set(['-i', '-o', '-e', '--input', '--output', '--error']),
  caffeinate: new Set(['-t', '-w']),
  chrt: new Set(['-T', '-P', '-D', '--sched-runtime', '--sched-period', '--sched-deadline']),
};
/** Operands a wrapper takes before the command it runs (`timeout 5 rm …`, `chrt 10 rm …`, `taskset 0x3 rm …`). */
const LEADING_OPERANDS: Record<string, number> = { timeout: 1, chrt: 1, taskset: 1 };
export const SHELLS = new Set(['sh', 'bash', 'zsh', 'dash', 'ksh']);
/**
 * Whitespace and shell operators inside quotes are shifted into this private-use block, so a quoted string stays one
 * word and splits nothing; `unhide` shifts them back, so a quoted script (`bash -c "…"`) can be checked as a command.
 */
const HIDDEN_BASE = 0xe000;
const OPERATOR = /[\s;&|()`<>$]/g;
const HIDDEN = /[\ue000-\ue07f]/g;
/** Stands for a heredoc in the masked command: `\u0002<index>` into the collected bodies. */
// oxlint-disable-next-line no-control-regex -- STX (\u0002) is the deliberate heredoc marker `mask` writes: a control character, so no word typed as shell text spells it by accident.
export const BODY = /^\u0002(\d+)$/;
const HEREDOC = /^<<(-?)[ \t]*(['"]?)([\w.-]+)\2/;
/** Separators between simple commands; captured so a pipe into a shell can be recognised. */
export const SEPARATOR = /(\|\||&&|[;&|\n()`]|\$\()/;

export const hide = (text: string): string => text.replace(OPERATOR, (char) => String.fromCharCode(HIDDEN_BASE + char.charCodeAt(0)));
export const unhide = (text: string): string => text.replace(HIDDEN, (char) => String.fromCharCode(char.charCodeAt(0) - HIDDEN_BASE));
export const base = (token: string): string => token.slice(token.lastIndexOf('/') + 1);

/** A heredoc whose body `mask` has yet to read: its delimiter, whether `<<-` strips leading tabs, and its slot in the bodies. */
interface Heredoc {
  tag: string;
  tabs: boolean;
  index: number;
  literal: boolean;
}
/** One pass of `mask`: the command, the bodies collected so far, and the heredocs whose bodies are still to come. */
interface Scan {
  command: string;
  bodies: string[];
  heredocs: Heredoc[];
}
/** What one construct contributes to the masked command, and where the next one starts. */
interface Step {
  text: string;
  next: number;
}

/**
 * The command with quotes removed (their content kept as one word, with operators neutralised) and each heredoc
 * replaced by a `\u0002<index>` word whose body is pushed onto `bodies`: what is left is what the shell splits into
 * commands.
 */
export function mask(command: string, bodies: string[]): string {
  const scan: Scan = { command, bodies, heredocs: [] };
  let out = '';
  let index = 0;
  while (index < command.length) {
    const step = maskStep(scan, index);
    out += step.text;
    index = step.next;
  }
  return out;
}

/** The construct that starts at `index`: heredoc bodies after a line break, an escape, a quoted string, a heredoc, or one character. */
function maskStep(scan: Scan, index: number): Step {
  const char = scan.command[index];
  if (char === '\n' && scan.heredocs.length > 0) return heredocBodies(scan, index);
  if (char === '\\') return escaped(scan.command, index);
  if (char === "'") return singleQuoted(scan.command, index);
  if (char === '"') return doubleQuoted(scan, index);
  return heredocStart(scan, index) ?? { text: char, next: index + 1 };
}

/** Each substitution body masked as separate commands. */
const maskedSubstitutions = (inners: string[], bodies: string[]): string => inners.map((inner) => `;${mask(inner, bodies)};`).join('');

/**
 * The line break at `index` and the heredoc bodies after it: body lines up to each pending delimiter, in order. The
 * shell expands an unquoted heredoc body: its substitutions run whether or not the body feeds a shell.
 */
function heredocBodies(scan: Scan, index: number): Step {
  const { bodies } = scan;
  const { read, next } = readBodies(scan, index + 1);
  const expanded = read.filter(({ literal }) => !literal).map(({ index: body }) => maskedSubstitutions(substitutionsIn(bodies[body] ?? ''), bodies));
  return { text: `\n${expanded.join('')}`, next };
}

/** Reads body lines from `start` into `bodies` until every pending heredoc is closed or the command ends (the last one then takes the rest). */
function readBodies({ command, bodies, heredocs }: Scan, start: number): { read: Heredoc[]; next: number } {
  let index = start;
  const lines: string[] = [];
  const read: Heredoc[] = [];
  while (heredocs.length > 0 && index < command.length) {
    const end = command.indexOf('\n', index);
    const stop = end < 0 ? command.length : end;
    const line = command.slice(index, stop);
    index = stop + 1;
    const { tag, tabs, index: body } = heredocs[0];
    if ((tabs ? line.replace(/^\t+/, '') : line) === tag) {
      bodies[body] = lines.splice(0).join('\n');
      read.push(heredocs.shift()!);
    } else lines.push(line);
  }
  if (heredocs.length > 0) {
    bodies[heredocs[0].index] = lines.join('\n');
    read.push(heredocs[0]);
  }
  return { read, next: index };
}

/** A backslash escape: an escaped line break joins the lines, any other escaped character is kept as word text. */
function escaped(command: string, index: number): Step {
  const next = command[index + 1] ?? '';
  return { text: next === '\n' ? ' ' : hide(next), next: index + 2 };
}

/** A single-quoted string: its content as one word (an empty string still a word). */
function singleQuoted(command: string, index: number): Step {
  const end = command.indexOf("'", index + 1);
  const stop = end < 0 ? command.length : end;
  return { text: hide(command.slice(index + 1, stop)) || hide(' '), next: stop + 1 };
}

/** A double-quoted string: its content as one word, then its `$(…)` and backtick substitutions, which run even inside double quotes, as separate commands. */
function doubleQuoted({ command, bodies }: Scan, index: number): Step {
  let body = '';
  const substitutions: string[] = [];
  let cursor = index + 1;
  while (cursor < command.length && command[cursor] !== '"') {
    if (command[cursor] === '\\' && cursor + 1 < command.length) {
      body += command[cursor + 1];
      cursor += 2;
    } else if (command.startsWith('$(', cursor) || command[cursor] === '`') {
      const end = substitutionEnd(command, cursor);
      substitutions.push(command.slice(cursor + (command[cursor] === '`' ? 1 : 2), end));
      cursor = end + 1;
    } else body += command[cursor++];
  }
  return { text: (hide(body) || hide(' ')) + maskedSubstitutions(substitutions, bodies), next: cursor + 1 };
}

/** A heredoc operator (`<<TAG`, `<<-TAG`, quoted tags; not `<<<`): a `\u0002<index>` word, its body read after the line ends. */
function heredocStart(scan: Scan, index: number): Step | undefined {
  const { command, bodies } = scan;
  const heredoc = command[index] === '<' && command[index - 1] !== '<' ? HEREDOC.exec(command.slice(index)) : null;
  if (!heredoc || command[index + 2] === '<') return undefined;
  scan.heredocs.push({ tag: heredoc[3], tabs: heredoc[1] === '-', index: bodies.length, literal: heredoc[2] !== '' });
  bodies.push('');
  return { text: ` \u0002${bodies.length - 1} `, next: index + heredoc[0].length };
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

/** The bodies of the unescaped `$(…)` and backtick substitutions in `text` (an unquoted heredoc body). */
function substitutionsIn(text: string): string[] {
  const found: string[] = [];
  for (let at = 0; at < text.length; at++) {
    if (text[at] === '\\') at++;
    else if (text.startsWith('$(', at) || text[at] === '`') {
      const end = substitutionEnd(text, at);
      found.push(text.slice(at + (text[at] === '`' ? 1 : 2), end));
      at = end;
    }
  }
  return found;
}

/** A simple command's words, past wrappers (their options, option values and leading operands) and `VAR=value`. */
export function words(segment: string): string[] {
  const tokens = segment.trim().split(/\s+/).filter(Boolean);
  let wrapper: string | undefined;
  let operands = 0;
  while (tokens.length > 0) {
    const token = tokens[0];
    if (WRAPPERS.has(base(token))) {
      wrapper = base(token);
      operands = LEADING_OPERANDS[wrapper] ?? 0;
      tokens.shift();
    } else if (/^\w+=/.test(token)) {
      tokens.shift();
    } else if (token.startsWith('-') && tokens.length > 1) {
      tokens.shift();
      if (wrapper && OPTION_ARGS[wrapper]?.has(token)) tokens.shift();
    } else if (operands > 0 && tokens.length > 1) {
      operands--;
      tokens.shift();
    } else break;
  }
  return tokens;
}

/** For a shell command: the script it runs from `-c`, or whether it reads its script from standard input. */
export function shellScript(tokens: string[]): { script?: string; stdin: boolean } {
  for (let at = 1; at < tokens.length; at++) {
    const token = tokens[at];
    if (token === '--') return { stdin: at + 1 >= tokens.length };
    if (/^-[a-zA-Z]*c[a-zA-Z]*$/.test(token)) return { script: tokens[at + 1] ?? '', stdin: false };
    if (/^-[a-zA-Z]*s/.test(token)) return { stdin: true };
    // Option values and redirection targets (`-o pipefail`, `> log`, `<<< text`) are not a script file.
    if (/^[-+][oO]$/.test(token) || /^\d*(?:<<<|<|>>?|&>)$/.test(token)) at++;
    else if (!/^[-+]/.test(token) && !BODY.test(token) && !/^\d*[<>&]/.test(token)) return { stdin: false };
  }
  return { stdin: true };
}

/** Text a command feeds to the next one's standard input: `echo`/`printf` arguments, or its own heredoc or herestring. */
export function stdinOf(tokens: string[], bodies: string[]): string[] {
  const fed: string[] = [];
  tokens.forEach((token, at) => {
    const body = BODY.exec(token);
    if (body) fed.push(bodies[Number(body[1])] ?? '');
    else if (token === '<<<') fed.push(tokens[at + 1] ?? '');
    else if (token.startsWith('<<<')) fed.push(token.slice(3));
  });
  return fed;
}
