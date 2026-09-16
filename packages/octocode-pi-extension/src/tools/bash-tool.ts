/**
 * Octocode `bash` — same-name override of Pi's built-in bash.
 * Keeps full shell power for git/builds/sed, but blocks redirects / tee /
 * cp|mv destinations that escape Octocode path-guard roots.
 */
import fs, { constants } from 'node:fs';
import { access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
import path from 'node:path';
import { getShellConfig } from '@earendil-works/pi-coding-agent';
import type { ToolCallResult, ToolDefinition } from '../types.js';

import { assertPathAllowed } from './path-guard.js';
import { classifySensitiveCommand, requestApproval, type ApprovalRequest } from './approval.js';
import type { PiContext } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import {
  JobManager,
  BASH_BG_DEFAULT_TIMEOUT_S,
  BG_STATUS_ICON, formatBgElapsed,
  projectBackgroundJobs,
  type BgJob,
} from './bash-bg-tool.js';
import { runtimeStoreFor } from './runtime-renderer.js';
import { chunkReadHint, writeEphemeralToolOutput } from './ephemeral-tool-output.js';
import { buildAwarenessCliEnvironment } from './awareness-cli-context.js';
import { BASH_CONTEXT_MAX_CHARS, renderBashCall, renderBashResult } from './bash-renderer.js';

export { BASH_CONTEXT_MAX_CHARS } from './bash-renderer.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

/** Head chars preserved in head+tail split (command invocation / early output). */
export const BASH_HEAD_CHARS = 1_000;
/** Tail chars preserved in head+tail split (errors and final summaries appear at the end). */
export const BASH_TAIL_CHARS = 3_000;
/** Stop accumulating stdout/stderr beyond this to prevent OOM on runaway processes. */
export const BASH_RAW_ACCUMULATION_MAX = 150_000;
/** Hard safety ceiling for a single ephemeral bash log. Crossing it is explicit. */
export const BASH_OUTPUT_FILE_MAX_BYTES = 64 * 1024 * 1024;
/** Hard upper ceiling on the timeout field (1 hour). A larger value almost certainly
 * indicates an agent mistake (e.g. passing milliseconds instead of seconds).
 * The value is clamped silently and the clamp is surfaced in the result text. */
export const BASH_MAX_TIMEOUT_SEC = 3600;
const PLAN_MODE_MUTATING_BASH_RE = /(^|[;|&(`\n])\s*(?:sudo\s+)?(?:touch|mkdir|rm|rmdir|mv|cp|install|ln|chmod|chown|truncate|dd|sed\s+[^;|&\n]*\s-i\b|perl\s+[^;|&\n]*\s-i\b|node\s+(?:--[^\s]+\s+)*-[ep]\b|python3?\s+-c\b|ruby\s+-e\b)\b|>>?|\btee\b/i;

/** Catastrophic patterns we refuse even when paths look local. */
const BLOCKED_COMMAND_PATTERNS: RegExp[] = [
  // Disk/filesystem destruction
  /\brm\s+(-[a-zA-Z]*f[a-zA-Z]*\s+|--force\s+)*\/\s*$/m,
  /\brm\s+(-[a-zA-Z]*rf[a-zA-Z]*|-[a-zA-Z]*fr[a-zA-Z]*)\s+\/(\s|$)/,
  /\bmkfs\b/,
  /\bdd\s+.*\bof=\/dev\//,
  />\s*\/dev\/sd[a-z]/,
  // Power-state commands (shutdown, restart, halt) — matched only when they appear as
  // the command itself, not as an argument. Patterns: command-start or after a shell
  // separator (; & | ( { newline), optionally preceded by sudo/nohup/exec.
  // Case-insensitive to catch REBOOT, SHUTDOWN, etc.
  // Limitation: does not detect power commands inside backtick or $() subshells.
  /(^|[;|&({\n])\s*(?:sudo\s+|nohup\s+|exec\s+)*\s*(?:shutdown|reboot|halt|poweroff)\b/im,
];

/**
 * Extract likely write targets from a shell command for path-guard checks.
 * Best-effort — not a full shell parser. Misses are fail-open for non-redirect
 * commands; hits outside roots are blocked.
 */
export function bashLooksMutatingForPlanMode(command: string, cwd: string = process.cwd()): boolean {
  return extractBashWriteTargets(command, cwd).length > 0 || classifySensitiveCommand(command) !== null || PLAN_MODE_MUTATING_BASH_RE.test(command);
}

export function extractBashWriteTargets(command: string, cwd: string): string[] {
  const targets: string[] = [];
  const syntax = shellSyntaxView(command);
  const push = (raw: string) => {
    const cleaned = raw
      .trim()
      .replace(/^['"]|['"]$/g, '')
      .replace(/\\ /g, ' ');
    if (!cleaned || cleaned === '-' || cleaned.startsWith('&') || cleaned.startsWith('(')) return;
    // Skip /dev/null and process substitutions.
    // Strip trailing ')' chars — they close a surrounding $(...) subshell and are
    // never part of an unquoted redirect target in valid shell (e.g. `> /dev/null)`
    // inside `$(cmd > /dev/null)` would otherwise capture "/dev/null)").
    const noTrailingParen = cleaned.replace(/\)+$/, '');
    if (!noTrailingParen || noTrailingParen === '/dev/null' || noTrailingParen.startsWith('/dev/fd/')) return;
    targets.push(path.isAbsolute(cleaned) ? cleaned : path.resolve(cwd, cleaned));
  };

  // Redirects: > file, >> file, 2> file, &> file, exec > file.
  // Note: destinations containing shell variable references ($VAR, ${VAR}) are recorded
  // as the literal token (resolved relative to cwd if not absolute). assertBashCommandAllowed
  // rejects such tokens before the path guard via assertNoShellExpansionInWriteTargets;
  // this extractor's contract is unchanged — it still records the raw token for callers.
  // Exclude ')' so `> /dev/null)` inside $(...) doesn't capture the subshell-closing paren.
  const redirectRe = /(?:^|[\s;|&])(?:\d*)?>>?\s*([^\s;|&<>)]+)/g;
  let match: RegExpExecArray | null;
  while ((match = redirectRe.exec(command)) !== null) {
    if (syntax[match.index + match[0].indexOf('>')] !== '>') continue;
    push(match[1]!);
  }

  // tee [-a] file...
  const teeRe = /\btee\b["']?(?:\s+-a)?\s+([^\n;|&]+)/g;
  while ((match = teeRe.exec(command)) !== null) {
    teeRe.lastIndex = match.index + 1;
    if (!isShellExecutable(command, syntax, match.index)) continue;
    for (const part of tokenizeShellSegment(shellArguments(command, syntax, match.index + match[0].indexOf(match[1]!)))) {
      if (part.startsWith('-')) continue;
      push(part);
    }
  }

  // cp/mv ... dest (last non-flag arg) — only when dest looks like a path
  const copyRe = /\b(?:cp|mv|install)\b["']?(?:\s+-[a-zA-Z]+|\s+--[^\s]+)*\s+(.+)$/gm;
  while ((match = copyRe.exec(command)) !== null) {
    copyRe.lastIndex = match.index + 1;
    if (!isShellExecutable(command, syntax, match.index)) continue;
    const args = tokenizeShellSegment(shellArguments(command, syntax, match.index + match[0].indexOf(match[1]!))).filter((a) => !a.startsWith('-'));
    const dest = args[args.length - 1];
    if (dest) push(dest);
  }

  // In-place editors (sed -i, perl -i) write their file arguments directly,
  // bypassing shell redirects. Extract those files so the guard sees them.
  // Opaque interpreters (node -e, python -c) can still write arbitrary paths and
  // are not statically parseable — the tool description documents that gap.
  for (const seg of splitShellCommands(command, syntax)) {
    for (const file of extractInPlaceEditTargets(seg)) push(file);
  }

  return [...new Set(targets)];
}

/** Keep source offsets while hiding quoted data; command substitutions remain executable. */
function shellSyntaxView(command: string): string {
  const view = command.split('');
  let quote: string | null = null;
  const frames: Array<{ close: string; quote: string | null }> = [];
  for (let i = 0; i < command.length; i++) {
    const char = command[i]!;
    if (char === '\\' && quote !== "'") { view[i] = ' '; if (++i < command.length) view[i] = ' '; continue; }
    if (quote === "'") { view[i] = ' '; if (char === "'") quote = null; continue; }
    if (char === '$' && command[i + 1] === '(') {
      frames.push({ close: ')', quote }); quote = null; i++; continue;
    }
    if (char === '`') {
      if (frames.at(-1)?.close === '`') quote = frames.pop()!.quote;
      else { frames.push({ close: '`', quote }); quote = null; }
      continue;
    }
    if (!quote && char === '(') frames.push({ close: ')', quote: null });
    else if (!quote && char === ')' && frames.at(-1)?.close === ')') quote = frames.pop()!.quote;
    else if (char === quote) { view[i] = ' '; quote = null; }
    else if (!quote && (char === "'" || char === '"')) { quote = char; view[i] = ' '; }
    else if (quote) view[i] = ' ';
  }
  return view.join('');
}

function splitShellCommands(command: string, syntax: string): string[] {
  const parts: string[] = [];
  let start = 0;
  for (const match of syntax.matchAll(/[;|&\n]+/g)) {
    parts.push(command.slice(start, match.index));
    start = match.index + match[0].length;
  }
  parts.push(command.slice(start));
  return parts;
}

function shellArguments(command: string, syntax: string, start: number): string {
  const end = syntax.slice(start).search(/[;|&\n)]/);
  return command.slice(start, end < 0 ? undefined : start + end);
}

/** A noun such as `skill install` is an argument, not the system install executable. */
function isShellExecutable(command: string, syntax: string, position: number): boolean {
  if (syntax[position] !== command[position]) {
    // Quoting the executable itself is different from mentioning it in a body.
    const opening = command.slice(0, position).match(/(["'])([^\s"']*\/)?$/);
    const word = command.slice(position).match(/^\w+/)?.[0];
    if (!opening || !word || command[position + word.length] !== opening[1]) return false;
    position -= opening[0].length;
  }
  const before = syntax.slice(0, position);
  const boundary = Math.max(...[';', '|', '&', '\n', '(', '{', '}', '`'].map(char => before.lastIndexOf(char))) + 1;
  const prefix = command.slice(boundary, position).replace(/(?:^|\s)\S*\/$/, '').trim();
  const tokens = tokenizeShellSegment(prefix);
  let i = 0;
  while (i < tokens.length) {
    if (/^[A-Za-z_]\w*=/.test(tokens[i]!)) { i++; continue; }
    const wrapper = tokens[i++];
    if (['then', 'do', 'else', 'elif', 'if', 'while', 'until', '!', 'time'].includes(wrapper!)) continue;
    if (!['sudo', 'command', 'exec', 'env'].includes(wrapper!)) return false;
    while (tokens[i]?.startsWith('-')) {
      const option = tokens[i++];
      if (['-u', '-g', '-h', '-p', '-C', '-T', '-a', '--user', '--group', '--host', '--prompt', '--unset'].includes(option!)) i++;
    }
  }
  return true;
}

/**
 * Split a shell segment into tokens, keeping single/double-quoted runs intact.
 * Best-effort — enough to isolate the file arguments of an in-place editor.
 */
function tokenizeShellSegment(seg: string): string[] {
  const tokens: string[] = [];
  const re = /"(?:[^"\\]|\\.)*"|'[^']*'|[^\s]+/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(seg)) !== null) tokens.push(m[0]);
  return tokens;
}

function stripOuterQuotes(t: string): string {
  return t.replace(/^['"]/, '').replace(/['"]$/, '');
}

/**
 * Extract file targets written by `sed -i` / `perl -i` in a single command
 * segment. Returns [] when no in-place editor is present.
 *
 * Correctly separates the SCRIPT from the FILE operands across dialects, so a
 * script token (e.g. the BSD/macOS `sed -i '' '/^re/d' file` address, which
 * starts with `/`) is never mistaken for an absolute output path:
 *  - GNU `sed -i 's/a/b/' f` / attached suffix `sed -i.bak 's/a/b/' f`
 *  - BSD `sed -i '' 's/a/b/' f` (separate empty backup-suffix arg)
 *  - explicit script via `-e <script>` / script file via `-f <file>` (skipped)
 *  - `perl -i -pe 's/a/b/' f` (script is the token after the flag bundle)
 */
function extractInPlaceEditTargets(seg: string): string[] {
  const tokens = tokenizeShellSegment(seg);
  const cmdIdx = tokens.findIndex((t) => t === 'sed' || t === 'perl');
  if (cmdIdx === -1) return [];
  const rest = tokens.slice(cmdIdx + 1);
  if (!rest.some((t) => /^-i/.test(t) || /^--in-place/.test(t))) return [];

  // An explicit `-e`/`-f` script means every positional token is a FILE (no
  // inline positional script to skip).
  const hasExplicitScript = rest.some((t) => t === '-e' || t === '--expression' || t === '-f' || t === '--file');

  const files: string[] = [];
  let inlineScriptSeen = false;
  for (let i = 0; i < rest.length; i++) {
    const t = rest[i]!;
    if (t.startsWith('-')) {
      if (t === '-e' || t === '--expression' || t === '-f' || t === '--file') {
        i++; // the next token is a script / script-file, not an output file
        continue;
      }
      if (t === '-i' || t === '--in-place') {
        // BSD sed takes a SEPARATE backup-suffix arg (often ''); GNU -i takes
        // none. Consume the next token only when it is unambiguously a suffix
        // (empty or dotted), never a script or a real filename.
        const next = rest[i + 1] !== undefined ? stripOuterQuotes(rest[i + 1]!) : undefined;
        if (next !== undefined && (next === '' || /^\.[\w.-]*$/.test(next))) i++;
        continue;
      }
      continue; // other flags (-n, -E, -pe, …)
    }
    if (!hasExplicitScript && !inlineScriptSeen) {
      inlineScriptSeen = true; // first positional is the inline script, not a file
      continue;
    }
    files.push(t);
  }
  return files;
}

export function classifyEnvExfilCommand(command: string): ApprovalRequest | null {
  const cmd = command.trim();
  // Direct env dump: env, printenv, set (standalone), declare/typeset (ksh/zsh alias for
  // declare — standalone or with flags), export -p (print all exported vars),
  // compgen -v (bash built-in listing all variable names).
  const obviousEnvironmentDump =
    /(^|[;|&(`\n])\s*(?:env|printenv)(?:\s|$)/i.test(cmd) ||
    /(^|[;|&(`\n])\s*(?:set|declare|typeset)(?:\s|$)/i.test(cmd) ||
    /(^|[;|&(`\n])\s*export\s+-[a-zA-Z]*p[a-zA-Z]*(?:\s|$)/i.test(cmd) ||
    /(^|[;|&(`\n])\s*compgen\s+-[a-zA-Z]*v[a-zA-Z]*(?:\s|$)/i.test(cmd) ||
    /\/proc\/(?:self|\d+)\/environ\b/.test(cmd);
  const obviousSecretEcho = /\b(?:echo|printf)\b[^;|&\n]*(?:\$\{?[A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|PASS|KEY|CREDENTIAL|AUTH)[A-Z0-9_]*\}?)/i.test(cmd);
  if (!obviousEnvironmentDump && !obviousSecretEcho) return null;
  return {
    actionClass: 'system',
    title: 'Expose inherited environment variables',
    detail: cmd,
  };
}

/** Detects $VAR, ${VAR}, $(cmd), or backtick substitution in a write-target token. */
const SHELL_EXPANSION_RE = /(?:\$[\w{(]|`)/;

function containsShellExpansion(token: string): boolean {
  return SHELL_EXPANSION_RE.test(token);
}

/**
 * Reject redirect / tee / cp|mv destinations that contain shell variable or command
 * substitution.  The path guard cannot verify these without executing the shell, so
 * we hard-error with a clear diagnostic asking for a literal path.
 */
function assertNoShellExpansionInWriteTargets(command: string): void {
  const syntax = shellSyntaxView(command);
  const blocked = (raw: string, kind: string): never => {
    throw new Error(
      `bash blocked: ${kind} target "${raw}" contains a shell variable or command substitution — ` +
      `the real destination cannot be verified by the path guard without executing the shell. ` +
      `Use a literal path instead (e.g. replace "$OUTFILE" with the explicit "/path/to/file").`,
    );
  };

  // Exclude ')' so `> /dev/null)` inside $(...) doesn't capture the subshell-closing paren.
  const redirectRe = /(?:^|[\s;|&])(?:\d*)?>>?\s*([^\s;|&<>)]+)/g;
  let m: RegExpExecArray | null;
  while ((m = redirectRe.exec(command)) !== null) {
    if (syntax[m.index + m[0].indexOf('>')] !== '>') continue;
    const raw = (m[1] ?? '').replace(/^['"]|['"]$/g, '').replace(/\)+$/, '');
    if (!raw || raw === '-' || raw.startsWith('&') || raw === '/dev/null' || raw.startsWith('/dev/fd/')) continue;
    if (containsShellExpansion(raw)) blocked(raw, 'redirect');
  }

  const teeRe = /\btee\b["']?(?:\s+-a)?\s+([^\n;|&]+)/g;
  while ((m = teeRe.exec(command)) !== null) {
    teeRe.lastIndex = m.index + 1;
    if (!isShellExecutable(command, syntax, m.index)) continue;
    for (const part of tokenizeShellSegment(shellArguments(command, syntax, m.index + m[0].indexOf(m[1]!)))) {
      if (!part || part.startsWith('-')) continue;
      const raw = part.replace(/^['"]|['"]$/g, '');
      if (containsShellExpansion(raw)) blocked(raw, 'tee');
    }
  }

  const copyRe = /\b(?:cp|mv|install)\b["']?(?:\s+-[a-zA-Z]+|\s+--[^\s]+)*\s+(.+)$/gm;
  while ((m = copyRe.exec(command)) !== null) {
    copyRe.lastIndex = m.index + 1;
    if (!isShellExecutable(command, syntax, m.index)) continue;
    const args = tokenizeShellSegment(shellArguments(command, syntax, m.index + m[0].indexOf(m[1]!))).filter((a) => !a.startsWith('-'));
    const dest = args.at(-1);
    if (dest && containsShellExpansion(dest)) blocked(dest, 'copy/move destination');
  }
}

export function assertBashCommandAllowed(command: string, cwd: string): void {
  for (const pattern of BLOCKED_COMMAND_PATTERNS) {
    if (pattern.test(command)) {
      throw new Error(
        `bash blocked: command matches a catastrophic pattern. ` +
          `Refusing to run. Rewrite the command to a safer form.`,
      );
    }
  }
  // Reject ambiguous write targets with shell expansion before the path guard.
  assertNoShellExpansionInWriteTargets(command);
  for (const target of extractBashWriteTargets(command, cwd)) {
    assertPathAllowed(target, cwd, 'bash write');
  }
}

export interface BashFormattedOutput {
  /** Model-visible text (≤ BASH_CONTEXT_MAX_CHARS + label overhead). Always a single block. */
  text: string;
  /** Whether the output was truncated (head+tail selection applied). */
  truncated: boolean;
  /** Original raw character count before truncation. */
  totalChars: number;
  /** Path to the disk-spilled full output file (present when truncated and write succeeded). */
  tempFilePath?: string;
}

function safeSliceTail(text: string, chars: number): string {
  let start = Math.max(0, text.length - chars);
  if (start > 0 && (text.charCodeAt(start) & 0xFC00) === 0xDC00) start += 1;
  return text.slice(start);
}

/**
 * Format bash output for model context injection.
 *
 * Returns a SINGLE bounded content block (≤ BASH_CONTEXT_MAX_CHARS + label overhead).
 * When the raw output exceeds the limit:
 *  - head+tail selection preserves the first BASH_HEAD_CHARS and last BASH_TAIL_CHARS chars
 *  - the full output is disk-spilled and the path is included in the returned text
 *  - slice boundaries are adjusted to avoid splitting Unicode surrogate pairs
 */
export async function formatBashOutput(raw: string, existingOutputPath?: string, originalChars = raw.length): Promise<BashFormattedOutput> {
  let tempFilePath = existingOutputPath;
  if (!tempFilePath) {
    try {
      tempFilePath = writeEphemeralToolOutput(raw, { toolName: 'bash', extension: 'log' });
    } catch {
      tempFilePath = undefined;
    }
  }
  if (originalChars <= BASH_CONTEXT_MAX_CHARS && raw.length <= BASH_CONTEXT_MAX_CHARS) {
    return { text: raw, truncated: false, totalChars: originalChars, tempFilePath };
  }

  // Surrogate-safe head boundary: don't end on a high surrogate
  let headEnd = Math.min(BASH_HEAD_CHARS, raw.length);
  if (headEnd < raw.length && (raw.charCodeAt(headEnd - 1) & 0xFC00) === 0xD800) headEnd--;

  // Surrogate-safe tail boundary: don't start on a low surrogate
  let tailStart = Math.max(raw.length - BASH_TAIL_CHARS, headEnd);
  if (tailStart > 0 && tailStart < raw.length && (raw.charCodeAt(tailStart) & 0xFC00) === 0xDC00) tailStart++;

  const head = raw.slice(0, headEnd);
  const tail = raw.slice(tailStart);
  const omitted = Math.max(0, originalChars - headEnd - tail.length);

  const notice =
    `[… ${omitted.toLocaleString()} chars omitted — ` +
    `first ${headEnd.toLocaleString()} + last ${tail.length.toLocaleString()} of ${originalChars.toLocaleString()} chars shown` +
    `${tempFilePath ? `. Full output: ${tempFilePath}` : ''}]` +
    `${tempFilePath ? `\n${chunkReadHint(tempFilePath)}` : ''}`;

  const text = `${head}\n\n${notice}\n\n${tail}`;
  return { text, truncated: true, totalChars: originalChars, tempFilePath };
}

async function runBash(
  command: string,
  cwd: string,
  timeoutSec: number | undefined,
  outputPath: string,
  signal?: AbortSignal,
  env: NodeJS.ProcessEnv = process.env,
): Promise<{ stdout: string; stderr: string; recentTail: string; stdoutChars: number; stderrChars: number; code: number | null; signal: NodeJS.Signals | null; aborted: boolean; previewCapped: boolean; fileCapped: boolean }> {
  await access(cwd, constants.F_OK).catch(() => {
    throw new Error(`Working directory does not exist: ${cwd}\nCannot execute bash commands.`);
  });
  // Cancellation is reported as a normal failed tool result, not a batch
  // execution exception. This also closes the small race where cancellation
  // arrives after preflight but before the child process is spawned.
  if (signal?.aborted) {
    return {
      stdout: '',
      stderr: '',
      recentTail: '',
      stdoutChars: 0,
      stderrChars: 0,
      code: null,
      signal: null,
      aborted: true,
      previewCapped: false,
      fileCapped: false,
    };
  }

  const shell = getShellConfig().shell;
  return await new Promise((resolve, reject) => {
    const outputFd = fs.openSync(outputPath, 'a');
    let outputBytes = 0;
    let outputFileCapped = false;
    let outputClosed = false;
    const closeOutput = () => {
      if (outputClosed) return;
      outputClosed = true;
      try { fs.closeSync(outputFd); } catch { /* best effort */ }
    };
    const persistChunk = (chunk: Buffer) => {
      if (outputBytes >= BASH_OUTPUT_FILE_MAX_BYTES) {
        outputFileCapped = true;
        return;
      }
      const kept = chunk.subarray(0, BASH_OUTPUT_FILE_MAX_BYTES - outputBytes);
      try {
        fs.writeSync(outputFd, kept);
        outputBytes += kept.length;
        if (kept.length < chunk.length) outputFileCapped = true;
      } catch {
        outputFileCapped = true;
      }
    };
    const child = spawn(shell, ['-lc', command], {
      cwd,
      env,
      detached: process.platform !== 'win32',
    });
    let stdout = '';
    let stderr = '';
    let recentTail = '';
    let stdoutChars = 0;
    let stderrChars = 0;
    let settled = false;
    const killChild = (sig: NodeJS.Signals) => {
      try {
        if (process.platform !== 'win32' && child.pid) {
          process.kill(-child.pid, sig);
        } else {
          child.kill(sig);
        }
      } catch {
        try {
          child.kill(sig);
        } catch {
          /* ignore */
        }
      }
    };
    const terminateChild = () => {
      killChild('SIGTERM');
      // A SIGTERM-trapping or uninterruptible child would otherwise leave the
      // execute promise pending forever (abort has no other backstop).
      const escalate = setTimeout(() => {
        if (!settled) killChild('SIGKILL');
      }, 2000);
      escalate.unref?.();
    };
    const timer =
      timeoutSec && timeoutSec > 0
        ? setTimeout(() => {
            terminateChild();
            if (!settled) {
              settled = true;
              reject(new Error(`bash timed out after ${timeoutSec}s`));
            }
          }, timeoutSec * 1000)
        : null;

    let aborted = false;
    const onAbort = () => {
      aborted = true;
      terminateChild();
    };
    signal?.addEventListener('abort', onAbort, { once: true });

    // Decode per-stream so multibyte UTF-8 sequences split across chunk
    // boundaries are not corrupted; flush trailing partial bytes on close.
    const outDec = new StringDecoder('utf8');
    const errDec = new StringDecoder('utf8');
    let accumulated = 0;
    let outputCapped = false;
    const rememberTail = (text: string) => {
      recentTail = safeSliceTail(recentTail + text, BASH_TAIL_CHARS);
    };
    child.stdout?.on('data', (chunk: Buffer) => {
      persistChunk(chunk);
      const text = outDec.write(chunk);
      stdoutChars += text.length;
      rememberTail(text);
      // Always read to drain the pipe so the child process can exit.
      // Only accumulate up to BASH_RAW_ACCUMULATION_MAX to prevent OOM.
      if (accumulated < BASH_RAW_ACCUMULATION_MAX) {
        const remaining = BASH_RAW_ACCUMULATION_MAX - accumulated;
        stdout += text.slice(0, remaining);
        accumulated += Math.min(text.length, remaining);
        if (text.length > remaining || accumulated >= BASH_RAW_ACCUMULATION_MAX) outputCapped = true;
      }
    });
    child.stderr?.on('data', (chunk: Buffer) => {
      persistChunk(chunk);
      const text = errDec.write(chunk);
      stderrChars += text.length;
      rememberTail(text);
      if (accumulated < BASH_RAW_ACCUMULATION_MAX) {
        const remaining = BASH_RAW_ACCUMULATION_MAX - accumulated;
        stderr += text.slice(0, remaining);
        accumulated += Math.min(text.length, remaining);
        if (text.length > remaining || accumulated >= BASH_RAW_ACCUMULATION_MAX) outputCapped = true;
      }
    });
    child.on('error', (err) => {
      if (timer) clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
      closeOutput();
      if (!settled) {
        settled = true;
        reject(err);
      }
    });
    child.on('close', (code, sig) => {
      if (timer) clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
      closeOutput();
      if (!settled) {
        settled = true;
        const stdoutEnd = outDec.end();
        const stderrEnd = errDec.end();
        stdout += stdoutEnd;
        stderr += stderrEnd;
        stdoutChars += stdoutEnd.length;
        stderrChars += stderrEnd.length;
        rememberTail(stdoutEnd);
        rememberTail(stderrEnd);
        resolve({ stdout, stderr, recentTail, stdoutChars, stderrChars, code, signal: sig, aborted, previewCapped: outputCapped, fileCapped: outputFileCapped });
      }
    });
  });
}

export function registerBashTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): JobManager {
  let bgRuntimeCtx: PiContext | undefined;
  const manager = new JobManager(() => {
    const store = runtimeStoreFor(bgRuntimeCtx);
    if (!store) return;
    store.getState().setBackgroundJobs(projectBackgroundJobs(manager.list()));
  });

  const parameters = buildQueryEnvelopeSchema(
    z.looseObject({
      command: z.string().optional().describe(
        'Command to run; required unless action is set.',
      ),
      timeout: z.number().int().min(1).optional().describe(
        `Seconds; capped at ${BASH_MAX_TIMEOUT_SEC}.`,
      ),
      background: z.boolean().optional().describe(
        `Non-blocking; returns jobId. Default ${BASH_BG_DEFAULT_TIMEOUT_S}s.`,
      ),
      title: z.string().optional().describe('Background label.'),
      action: z.enum(['status', 'output', 'kill', 'list']).optional().describe(
        'Manage jobs; status/output/kill need jobId.',
      ),
      jobId: z.string().optional().describe('Job target; required for action: status/output/kill.'),
      outputOffset: z.number().optional().describe('Output line offset.'),
      lines: z.number().optional().describe('Output line limit.'),
    }),
    { reasoningDescription: 'Why.', allowParallel: false },
  );

  registerFn(pi, registeredToolNames, {
    name: 'bash',
    label: 'bash (Octocode)',
    description:
      DIRECT_TOOL_DESCRIPTIONS.bash!,
    promptSnippet: 'Run bounded builds, tests, package commands, and debugging.',
    promptGuidelines: [
      'Set timeout; use non-interactive flags (npx -y). Isolate slow/network commands.',
    ],
    parameters,
    async execute(
      toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
      ctx?: PiContext,
    ): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      const batchResult = await executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        preflight(query) {
          const action = query['action'] as string | undefined;
          if (action) {
            // Job management: no command needed
            if (!['status', 'output', 'kill', 'list'].includes(action))
              throw new Error(`Unknown action '${action}'. Valid: status | output | kill | list`);
            if (action !== 'list' && !query['jobId'])
              throw new Error(`jobId is required for action=${action}`);
            return;
          }
          if (typeof query['command'] !== 'string' || query['command'].trim().length === 0) {
            throw new Error('command must be a non-empty string.');
          }
          assertBashCommandAllowed(query['command'], cwd);
        },
        async execute(query, _index, itemCallId) {
          bgRuntimeCtx = ctx;
          const action = query['action'] as string | undefined;

          // ── job management ────────────────────────────────────────────────
          if (action) {
            const jobId = query['jobId'] as string | undefined;

            if (action === 'list') {
              const all = manager.list();
              if (all.length === 0)
                return { content: [{ type: 'text' as const, text: 'No background jobs.' }], details: {} };
              const now  = Date.now();
              const rows = all.map(j =>
                `${BG_STATUS_ICON[j.status]} [${j.id}]  ${j.status.padEnd(9)}  ${formatBgElapsed(now - j.startedAt).padStart(6)}  ${j.title}`,
              );
              return { content: [{ type: 'text' as const, text: rows.join('\n') }], details: { count: all.length } };
            }

            const job = manager.get(jobId!);
            if (!job) return { content: [{ type: 'text' as const, text: `bash: job '${jobId}' not found — use action='list'` }], isError: true, details: { error: 'not found' } };

            if (action === 'status') {
              const now     = Date.now();
              const runtime = job.endedAt ? formatBgElapsed(job.endedAt - job.startedAt) : formatBgElapsed(now - job.startedAt);
              return {
                content: [{ type: 'text' as const, text:
                  `Job:     ${job.id}\nStatus:  ${BG_STATUS_ICON[job.status]} ${job.status}\nTitle:   ${job.title}\nRuntime: ${runtime}\nExit:    ${job.exitCode ?? '(still running)'}\nLog:     ${job.logPath}`,
                }],
                details: { jobId, status: job.status, exitCode: job.exitCode },
              };
            }
            if (action === 'output') {
              const { text, nextOffset } = manager.output(jobId!, query['outputOffset'] as number ?? 0, query['lines'] as number ?? 200);
              return { content: [{ type: 'text' as const, text }], details: { jobId, nextOffset, logPath: job.logPath, status: job.status } };
            }
            if (action === 'kill') {
              manager.kill(jobId!);
              return { content: [{ type: 'text' as const, text: `Killed job ${jobId} (${job.title})` }], details: { jobId, status: 'killed' } };
            }
            return { content: [{ type: 'text' as const, text: `Unknown action: ${action}` }], isError: true, details: {} };
          }

          const command = query['command'] as string;

          // ── background (non-blocking) ───────────────────────────────────────
          if (query['background']) {
            const rawBgTimeout = typeof query['timeout'] === 'number' ? query['timeout'] : BASH_BG_DEFAULT_TIMEOUT_S;
            const bgTimeout    = rawBgTimeout > 0 ? Math.max(1, rawBgTimeout) : 0;
            const bgTitle      = typeof query['title'] === 'string' ? query['title'] : undefined;
            const job: BgJob   = await manager.start(
              command, cwd, bgTimeout, bgTitle,
              (j) => {
                try {
                  const icon = BG_STATUS_ICON[j.status] ?? '?';
                  const msg  =
                    `${icon} bash (bg) job \`${j.id}\` ${j.status}\n` +
                    `Title:   ${j.title}\nRuntime: ${formatBgElapsed((j.endedAt ?? Date.now()) - j.startedAt)}\n` +
                    `Exit:    ${j.exitCode ?? 'n/a'}\nLog:     ${j.logPath}\n\n` +
                    `  bash queries=[{action:'output',jobId:'${j.id}',reasoning:'tail output'}]   # read log`;
                  (ctx as any)?.sendUserMessage?.(msg, { deliverAs: 'followUp' });
                } catch {}
              },
            );
            return {
              content: [{ type: 'text' as const, text:
                `✓ Background job \`${job.id}\` started\n` +
                `Title:   ${job.title}\nCommand: ${job.command}\nLog:     ${job.logPath}\n` +
                `Timeout: ${bgTimeout > 0 ? `${bgTimeout}s` : 'none'}\n\n` +
                `Continue other work — follow-up arrives on completion.\n` +
                `  bash queries=[{action:'output',jobId:'${job.id}',reasoning:'tail log'}]\n` +
                `  bash queries=[{action:'status',jobId:'${job.id}',reasoning:'check exit code'}]\n` +
                `  bash queries=[{action:'kill',  jobId:'${job.id}',reasoning:'stop it'}]`,
              }],
              details: { jobId: job.id, logPath: job.logPath, status: 'running' },
            };
          }

          // ── synchronous (existing path) ──────────────────────────────────────
          const rawTimeout = typeof query['timeout'] === 'number' && Number.isFinite(query['timeout'])
            ? query['timeout']
            : undefined;
          const timeoutClamped = rawTimeout !== undefined && rawTimeout > BASH_MAX_TIMEOUT_SEC;
          const timeout = rawTimeout !== undefined
            ? Math.min(Math.max(1, rawTimeout), BASH_MAX_TIMEOUT_SEC)
            : undefined;

          // Evaluate env-exfil independently — the previous `??` short-circuit let
          // classifySensitiveCommand silently mask classifyEnvExfilCommand, so a command
          // like `git push && env` never prompted for env-exfil even when git-write was
          // always-allowed. Check both and deduplicate by actionClass to avoid a redundant
          // prompt when both return the same class (e.g. 'system').
          const envExfil = classifyEnvExfilCommand(command);
          const sensitive = classifySensitiveCommand(command);
          const seenClasses = new Set<string>();
          for (const request of [envExfil, sensitive].filter(Boolean) as ApprovalRequest[]) {
            if (seenClasses.has(request.actionClass)) continue;
            seenClasses.add(request.actionClass);
            const outcome = await requestApproval(ctx, request, signal);
            if (!outcome.approved) {
              const why = outcome.interactive
                ? 'The user declined this action.'
                : 'This host is non-interactive, so consent could not be collected. Ask the user inline to confirm before retrying.';
              throw new Error(`bash blocked: "${request.title}" requires user approval. ${why}`);
            }
          }
          const outputPath = writeEphemeralToolOutput('', { toolName: 'bash', toolCallId: itemCallId, extension: 'log' });
          const { stdout, stderr, recentTail, stdoutChars, stderrChars, code, signal: killedBy, aborted, previewCapped, fileCapped } = await runBash(command, cwd, timeout, outputPath, signal, buildAwarenessCliEnvironment(ctx));
          // Label stderr separately when both streams have content so the agent can
          // distinguish stdout from stderr without losing the tail context.
          const stderrLabeled = stdout && stderr ? `[stderr]\n${stderr}` : stderr;
          let combined = [stdout, stderrLabeled].filter(Boolean).join('\n');
          if (timeoutClamped) {
            combined = `[Warning: timeout ${rawTimeout}s exceeded max ceiling of ${BASH_MAX_TIMEOUT_SEC}s — clamped to ${BASH_MAX_TIMEOUT_SEC}s]\n${combined}`;
          }
          if (previewCapped) {
            combined += `\n[Inline preview source capped at ${BASH_RAW_ACCUMULATION_MAX.toLocaleString()} chars; inspect the referenced log for later output]`;
            if (recentTail) combined += `\n[Actual process tail]\n${recentTail}`;
          }
          if (fileCapped) {
            combined += `\n[Referenced log hit the ${BASH_OUTPUT_FILE_MAX_BYTES.toLocaleString()}-byte safety ceiling; later process output was discarded]`;
          }
          const isError = aborted || code !== 0;
          const exitNote = aborted ? '(aborted)' : killedBy ? `(killed by ${killedBy})` : `(exit ${code ?? 'null'})`;
          const rawBody = isError && combined ? `${combined}\n${exitNote}` : combined;
          const formatted = await formatBashOutput(
            rawBody || exitNote,
            outputPath,
            stdoutChars + stderrChars + (isError && stdoutChars + stderrChars > 0 ? exitNote.length + 1 : 0),
          );
          return {
            content: [{ type: 'text' as const, text: formatted.text }],
            isError,
            details: {
              code,
              outputPath: formatted.tempFilePath,
              totalChars: formatted.totalChars,
              stdoutChars,
              stderrChars,
            },
          };
        },
      });
      return batchResult;
    },
    renderCall: renderBashCall,
    renderResult: renderBashResult,
  });
  return manager;
}
