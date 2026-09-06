import { truncateToWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
/**
 * Octocode `bash` — same-name override of Pi's built-in bash.
 * Keeps full shell power for git/builds/sed, but blocks redirects / tee /
 * cp|mv destinations that escape Octocode path-guard roots.
 */
/** Output lines shown per-query under a collapsed bash result row (tail of output). */
const BASH_COLLAPSED_LINES = 3;

import fs, { constants } from 'node:fs';
import { access } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
import path from 'node:path';
import { getShellConfig } from '@earendil-works/pi-coding-agent';
import type { TSchema, ToolCallResult, ToolDefinition, PiTheme } from '../types.js';

import { buildToolView, makeComponentRenderer } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { classifySensitiveCommand, requestApproval, type ApprovalRequest } from './approval.js';
import type { PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { chunkReadHint, writeEphemeralToolOutput } from './ephemeral-tool-output.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

/** Max chars injected into model context per bash call (single content block). */
export const BASH_CONTEXT_MAX_CHARS = 4_000;
/** Head chars preserved in head+tail split (command invocation / early output). */
export const BASH_HEAD_CHARS = 1_000;
/** Tail chars preserved in head+tail split (errors and final summaries appear at the end). */
export const BASH_TAIL_CHARS = 3_000;
/** Stop accumulating stdout/stderr beyond this to prevent OOM on runaway processes. */
export const BASH_RAW_ACCUMULATION_MAX = 150_000;
/** Hard safety ceiling for a single ephemeral bash log. Crossing it is explicit. */
export const BASH_OUTPUT_FILE_MAX_BYTES = 64 * 1024 * 1024;
const BASH_TOOL_DISPLAY_NAME = 'bash (Octocode)';

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
    push(match[1]!);
  }

  // tee [-a] file...
  const teeRe = /\btee\b(?:\s+-a)?\s+([^\n;|&]+)/g;
  while ((match = teeRe.exec(command)) !== null) {
    for (const part of match[1]!.trim().split(/\s+/)) {
      if (part.startsWith('-')) continue;
      push(part);
    }
  }

  // cp/mv ... dest (last non-flag arg) — only when dest looks like a path
  const copyRe = /\b(?:cp|mv|install)\b(?:\s+-[a-zA-Z]+|\s+--[^\s]+)*\s+(.+)$/gm;
  while ((match = copyRe.exec(command)) !== null) {
    const args = match[1]!.trim().split(/\s+/).filter((a) => !a.startsWith('-'));
    const dest = args[args.length - 1];
    if (dest) push(dest);
  }

  // In-place editors (sed -i, perl -i) write their file arguments directly,
  // bypassing shell redirects. Extract those files so the guard sees them.
  // Opaque interpreters (node -e, python -c) can still write arbitrary paths and
  // are not statically parseable — the tool description documents that gap.
  for (const seg of command.split(/[;|&\n]+/)) {
    for (const file of extractInPlaceEditTargets(seg)) push(file);
  }

  return [...new Set(targets)];
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
  const obviousEnvironmentDump = /(^|[;|&(`\n])\s*(?:env|printenv)(?:\s|$)/i.test(cmd) ||
    /(^|[;|&(`\n])\s*(?:set|declare)(?:\s|$)/i.test(cmd) ||
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
    const raw = (m[1] ?? '').replace(/^['"]|['"]$/g, '').replace(/\)+$/, '');
    if (!raw || raw === '-' || raw.startsWith('&') || raw === '/dev/null' || raw.startsWith('/dev/fd/')) continue;
    if (containsShellExpansion(raw)) blocked(raw, 'redirect');
  }

  const teeRe = /\btee\b(?:\s+-a)?\s+([^\n;|&]+)/g;
  while ((m = teeRe.exec(command)) !== null) {
    for (const part of (m[1] ?? '').trim().split(/\s+/)) {
      if (!part || part.startsWith('-')) continue;
      const raw = part.replace(/^['"]|['"]$/g, '');
      if (containsShellExpansion(raw)) blocked(raw, 'tee');
    }
  }

  const copyRe = /\b(?:cp|mv|install)\b(?:\s+-[a-zA-Z]+|\s+--[^\s]+)*\s+(.+)$/gm;
  while ((m = copyRe.exec(command)) !== null) {
    const args = (m[1] ?? '').trim().split(/\s+/).filter((a) => !a.startsWith('-'));
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

function smartBashView(text: string, maxChars: number): { lines: string[]; omittedChars: number } {
  if (text.length <= maxChars) return { lines: text.split('\n').filter((line) => line.length > 0), omittedChars: 0 };
  const markerReserve = 96;
  const retained = Math.max(2, maxChars - markerReserve);
  const headChars = Math.ceil(retained / 2);
  const tailChars = retained - headChars;
  const omittedChars = text.length - headChars - tailChars;
  const preview = [
    text.slice(0, headChars),
    `… ${omittedChars} chars hidden in UI only; complete bash output was delivered to the agent …`,
    text.slice(text.length - tailChars),
  ].join('\n');
  return { lines: preview.split('\n').filter((line) => line.length > 0), omittedChars };
}

/** Read a bounded head+tail window for the TUI without loading the full log. */
function readBashOutputForUi(file: string | undefined, maxChars: number): string {
  if (!file) return '';
  let fd: number | undefined;
  try {
    fd = fs.openSync(file, 'r');
    const size = fs.fstatSync(fd).size;
    const byteBudget = Math.max(512, maxChars);
    if (size <= byteBudget) {
      const bytes = Buffer.alloc(size);
      fs.readSync(fd, bytes, 0, size, 0);
      return bytes.toString('utf8');
    }
    const headBytes = Math.floor(byteBudget / 4);
    const tailBytes = byteBudget - headBytes;
    const head = Buffer.alloc(headBytes);
    const tail = Buffer.alloc(tailBytes);
    fs.readSync(fd, head, 0, headBytes, 0);
    fs.readSync(fd, tail, 0, tailBytes, Math.max(0, size - tailBytes));
    return `${head.toString('utf8')}\n… ${size - byteBudget} bytes hidden in UI; full output remains in ${file} …\n${tail.toString('utf8')}`;
  } catch {
    return '';
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

async function runBash(
  command: string,
  cwd: string,
  timeoutSec: number | undefined,
  outputPath: string,
  signal?: AbortSignal,
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
      env: process.env,
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
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  const querySchema = Type.Object(
    {
      command: Type.String({ description: 'Bash command to execute' }),
      timeout: Type.Optional(
        Type.Integer({ description: 'Timeout in seconds (optional, no default timeout)' }),
      ),
    },
    { additionalProperties: false },
  ) as TSchema;
  const parameters = buildQueryEnvelopeSchema(Type, querySchema, {
    reasoningDescription: 'Concise reason this shell command is necessary.',
    allowParallel: false,
  });

  registerFn(pi, registeredToolNames, {
    name: 'bash',
    label: 'bash (Octocode)',
    description:
      'Octocode custom bash tool. Pass one or more ordered operations in queries; every query requires concise reasoning. queryRunType is sequential-only: commands always run one-by-one in source order, never in parallel. Replaces Pi built-in bash with the same shell execution plus Octocode path-guard on redirect/tee/cp/mv and sed -i / perl -i in-place write targets (cwd / home / OS temp / ALLOWED_PATHS), a small blocklist of catastrophic commands, and approval for obvious environment-variable exfiltration commands. Batches are preflighted, non-transactional, and stop on the first runtime failure. Note: opaque interpreters (node -e, python -c) can still write arbitrary paths and are not guarded — prefer edit/write for file mutations; use bash for git, builds, tests, and bulk mechanical edits.',
    promptSnippet: 'Run shell commands with Octocode path-guard on write targets.',
    promptGuidelines: [
      'Octocode custom bash replaces Pi built-in bash; prefer file for ordinary creates, edits, and deletes.',
      'Use bash for git, builds, tests, package managers, and bulk mechanical edits (e.g. sed).',
      'Bash query batches are always sequential: each command completes before the next starts.',
      'Commands that obviously print inherited environment variables or secret-like env vars require approval; bash otherwise keeps the inherited environment.',
      'Redirects (>, >>, tee) and cp/mv destinations must stay inside the working directory, home, OS temp, or ALLOWED_PATHS.',
      'Do not use bash to bypass the file path-guard.',
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
          if (typeof query['command'] !== 'string' || query['command'].trim().length === 0) {
            throw new Error('command must be a non-empty string.');
          }
          assertBashCommandAllowed(query['command'], cwd);
        },
        async execute(query, _index, itemCallId) {
          const command = query['command'] as string;
          const timeout = typeof query['timeout'] === 'number' && Number.isFinite(query['timeout'])
            ? query['timeout']
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
            const outcome = await requestApproval(ctx, request);
            if (!outcome.approved) {
              const why = outcome.interactive
                ? 'The user declined this action.'
                : 'This host is non-interactive, so consent could not be collected. Ask the user inline to confirm before retrying.';
              throw new Error(`bash blocked: "${request.title}" requires user approval. ${why}`);
            }
          }
          const outputPath = writeEphemeralToolOutput('', { toolName: 'bash', toolCallId: itemCallId, extension: 'log' });
          const { stdout, stderr, recentTail, stdoutChars, stderrChars, code, signal: killedBy, aborted, previewCapped, fileCapped } = await runBash(command, cwd, timeout, outputPath, signal);
          let combined = [stdout, stderr].filter(Boolean).join('\n');
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
    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
      const input = queries[0] ?? {};
      const command = typeof input['command'] === 'string' ? input['command'] : '(missing command)';
      return buildToolView({ name: BASH_TOOL_DISPLAY_NAME, state: 'request', segments: [{ text: command, token: 'dim' }] }, theme);
    },
    renderResult: Object.assign(
      function renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
        if (opts.isPartial) {
          return buildToolView(() => ({ name: BASH_TOOL_DISPLAY_NAME, state: 'running', status: 'running…' }), theme);
        }
        const ok = !result.isError;
        const details = result.details as {
          code?: number | null;
          outputPath?: string;
          totalChars?: number;
          stdoutChars?: number;
          stderrChars?: number;
          queryRunType?: string;
          results?: Array<{ index: number; status: string; result?: { code?: number | null; outputPath?: string; totalChars?: number } }>;
        } | undefined;

        // Multi-query: render each query's output separately.
        // Full stdout/stderr lives in details.results[N].result (set by execute above).
        const queryResults =
          Array.isArray(details?.results) && details!.results.length > 1
            ? details!.results
            : null;

        if (queryResults) {
          const qCount = queryResults.length;
          return makeComponentRenderer((_props, { width: width }) => {
            const lines: string[] = buildToolView({
              name: BASH_TOOL_DISPLAY_NAME,
              state: ok ? 'success' : 'error',
              segments: [
                { text: `${qCount} quer${qCount === 1 ? 'y' : 'ies'}`, token: 'count' },
                { text: details?.queryRunType ?? 'sequential', token: 'muted' },
              ],
            }, theme).render(width);
            for (const qr of queryResults) {
              const qOk = qr.status === 'success';
              const qCode = qr.result?.code;
              const combined = readBashOutputForUi(qr.result?.outputPath, Math.max(512, Math.floor(BASH_CONTEXT_MAX_CHARS / qCount)));
              const allQLines = combined.split('\n').filter((l) => l.length > 0);
              lines.push(...buildToolView({
                name: `[${qr.index}]`,
                state: qOk ? 'success' : 'error',
                segments: [
                  { text: `exit ${qCode ?? 'null'}`, token: qOk ? 'dim' : 'error' },
                  { text: `${allQLines.length} line${allQLines.length === 1 ? '' : 's'}`, token: 'count' },
                ],
              }, theme).render(width));
              const expandedView = smartBashView(
                combined,
                Math.max(512, Math.floor(BASH_CONTEXT_MAX_CHARS / qCount)),
              );
              const shown = opts.expanded ? expandedView.lines : allQLines.slice(-BASH_COLLAPSED_LINES);
              const hidden = opts.expanded ? 0 : allQLines.length - shown.length;
              if (hidden > 0) {
                lines.push(truncateToWidth(paint(theme, 'muted', `    \u2026 ${hidden} more line${hidden === 1 ? '' : 's'}`), width));
              }
              for (const line of shown) {
                lines.push(truncateToWidth(qOk ? paint(theme, 'dim', `    ${line}`) : paint(theme, 'error', `    ${line}`), width));
              }
            }
            if (!opts.expanded) {
              lines.push(truncateToWidth(paint(theme, 'muted', '  ctrl+o to expand full output'), width));
            }
            return lines;
          }, undefined);
        }

        // Single query: status header + last N lines (tail is most useful for
        // build/test \u2014 errors and final summary appear at the end).
        const detailText = readBashOutputForUi(details?.outputPath, BASH_CONTEXT_MAX_CHARS);
        const text = detailText || result.content
          .filter((c) => c.type === 'text')
          .map((c) => c.text)
          .join('\n');
        const allLines = text.split('\n').filter((l) => l.length > 0);
        const code = details?.code;
        const expandedView = smartBashView(text, BASH_CONTEXT_MAX_CHARS);
        const shown = opts.expanded ? expandedView.lines : allLines.slice(-BASH_COLLAPSED_LINES);
        const hidden = opts.expanded ? 0 : allLines.length - shown.length;
        return buildToolView({
          name: BASH_TOOL_DISPLAY_NAME,
          state: ok ? 'success' : 'error',
          segments: [
            { text: `exit ${code ?? 'null'}`, token: ok ? 'dim' : 'error' },
            { text: `${allLines.length} line${allLines.length === 1 ? '' : 's'}`, token: 'count' },
          ],
          body: shown.map((line) => ({ text: line, token: ok ? 'dim' : 'error' })),
          hint: hidden > 0 ? `${hidden} more line${hidden === 1 ? '' : 's'} hidden · ctrl+o expands` : undefined,
        }, theme);
      },
      // Signal to guardedRenderResult that this renderer handles multi-query itself.
      { multiQueryAware: true },
    ),
  });
}
