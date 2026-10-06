import { spawn } from 'node:child_process';
import { KILL_GRACE_MS, killTree } from '../shared/process.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { isRecord, parseJson } from '../shared/util.js';
import type { HookCommand, HookEvent } from './config.js';

/** Context a hook may add to the conversation, as in the native host. */
export const HOOK_CONTEXT_MAX_BYTES = 16 * 1024;
const OUTPUT_MAX_BYTES = 64 * 1024;

interface HookRun {
  code: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

/** Runs `hook.command` through the shell with `input` as JSON on stdin. Never rejects. */
export function runHook(hook: HookCommand, input: Record<string, unknown>, cwd: string, env: NodeJS.ProcessEnv = {}, signal?: AbortSignal): Promise<HookRun> {
  return new Promise((resolve) => {
    // Detached on POSIX makes the shell a process-group leader, so a timeout can kill everything it started.
    const child = spawn(hook.command, { cwd, shell: true, detached: process.platform !== 'win32', env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    const out: Buffer[] = [];
    const err: Buffer[] = [];
    let outBytes = 0;
    let errBytes = 0;
    let timedOut = false;
    const collect = (chunks: Buffer[], size: number, chunk: Buffer): number => {
      if (size < OUTPUT_MAX_BYTES) chunks.push(chunk.subarray(0, OUTPUT_MAX_BYTES - size));
      return size + chunk.length;
    };
    child.stdout.on('data', (chunk: Buffer) => (outBytes = collect(out, outBytes, chunk)));
    child.stderr.on('data', (chunk: Buffer) => (errBytes = collect(err, errBytes, chunk)));
    let settled = false;
    let deadline: NodeJS.Timeout | undefined;
    const stop = () => {
      if (deadline) return;
      // The whole tree: the shell's children and grandchildren too, not only the shell.
      killTree(child.pid, KILL_GRACE_MS);
      // A grandchild that survives or holds stdio open must not hold the run: resolve anyway after the grace.
      deadline = setTimeout(() => finish(null), 2 * KILL_GRACE_MS);
      deadline.unref();
      child.unref();
    };
    const timer = setTimeout(() => {
      timedOut = true;
      stop();
    }, hook.timeoutMs);
    if (signal?.aborted) stop();
    else signal?.addEventListener('abort', stop, { once: true });
    const finish = (code: number | null, extra = '') => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearTimeout(deadline);
      for (const stream of [child.stdin, child.stdout, child.stderr]) stream.destroy();
      signal?.removeEventListener('abort', stop);
      resolve({ code, stdout: Buffer.concat(out).toString('utf8'), stderr: `${Buffer.concat(err).toString('utf8')}${extra}`, timedOut });
    };
    child.on('error', (error) => finish(null, error.message));
    child.on('close', (code) => finish(code));
    // A hook that ignores stdin closes the pipe early; that is not an error.
    child.stdin.on('error', () => undefined);
    child.stdin.end(JSON.stringify(input));
  });
}

export interface HookVerdict {
  /** Set when the hook blocks: the reason shown to the model. */
  block?: string;
  /** Text the hook adds to the conversation. */
  context?: string;
}

/**
 * Claude Code semantics: exit 2 blocks with stderr as the reason; exit 0 may print JSON with `decision: "block"`,
 * `continue: false`, `hookSpecificOutput.permissionDecision: "deny"` or `additionalContext`; plain SessionStart stdout
 * is context. Any other exit is a non-blocking error.
 */
export function interpret(event: HookEvent, run: HookRun): HookVerdict {
  // Hook output is untrusted text: it reaches the model and Pi's tool errors without escape sequences.
  const { block, context } = verdictOf(event, run);
  return { ...(block ? { block: sanitizeTerminalText(block) } : {}), ...(context ? { context: sanitizeTerminalText(context) } : {}) };
}

function verdictOf(event: HookEvent, run: HookRun): HookVerdict {
  // Notification hooks are side effects only: they cannot block or add context.
  if (run.timedOut || event === 'Notification') return {};
  if (run.code === 2) return { block: run.stderr.trim() || 'Blocked by a hook.' };
  if (run.code !== 0) return {};
  const value = parseJson(run.stdout);
  const json = isRecord(value) ? value : undefined;
  if (!json) return event === 'SessionStart' && run.stdout.trim() ? { context: run.stdout.trim() } : {};
  const specific = isRecord(json.hookSpecificOutput) ? json.hookSpecificOutput : {};
  const reason = [json.reason, json.stopReason, specific.permissionDecisionReason].find((value): value is string => typeof value === 'string' && value.trim() !== '');
  const context = typeof specific.additionalContext === 'string' && specific.additionalContext.trim() ? specific.additionalContext.trim() : undefined;
  // For Stop, `continue: false` means "stop for good", the opposite of blocking (which keeps the agent going).
  const blocked = json.decision === 'block' || (event !== 'Stop' && (json.continue === false || specific.permissionDecision === 'deny'));
  return { ...(blocked ? { block: reason ?? 'Blocked by a hook.' } : {}), ...(context ? { context } : {}) };
}

/** Caps hook context at 16 KB, on a character boundary. */
export function capContext(text: string, maxBytes = HOOK_CONTEXT_MAX_BYTES): string {
  if (Buffer.byteLength(text) <= maxBytes) return text;
  // Decoding a byte prefix leaves at most one replacement character where a code point was cut.
  const head = Buffer.from(text).subarray(0, maxBytes - 32).toString('utf8').replace(/\uFFFD$/u, '');
  return `${head}\n[hook output truncated]`;
}
