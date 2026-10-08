import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Usage } from '@earendil-works/pi-ai';
import { BROWSER_VISIBLE_ENV, envInt, HOOKS_ENV, MCP_ENV, SUBAGENT_ENV, TRUST_ROOT_ENV } from '../shared/env.js';
import { formatDuration } from '../shared/format.js';
import { killTree } from '../shared/process.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { addUsage, emptyUsage, isRecord, toolHint } from '../shared/util.js';
import { AGENT_COLLABORATE_ENV, AGENT_ID_ENV, AGENT_SCRATCH_ENV, AGENT_TASK_ENV, PARENT_ID_ENV, TEAM_WORKSPACE_ENV } from '../team/store.js';
import type { AgentProfile } from './profiles.js';
import { imagesOf } from './screenshots.js';
import { ReportTracker } from './report.js';

/** The child's place in the team: its own id, the agent that started it, and its task line. */
export interface Identity {
  id: string;
  parentId?: string;
  task?: string;
  /** Works with its sibling subagents (messages them directly), not only for its parent. */
  collaborate?: boolean;
  /** The parent's team workspace, for a child whose cwd is an isolated worktree (a different git root). */
  workspace?: string;
  /** Set only when the parent's project config is trusted: the repository whose stored decision an isolated child follows. */
  trustRoot?: string;
  /** Its handoff folder: files it writes there are read by the parent on demand instead of riding in its report. */
  scratch?: string;
}

/**
 * Child environment. A `visibleBrowser` profile (webLive) gets a visible Chrome on the persistent profile. The user's
 * command hooks stay off in children unless the profile sets `hooks: true`: a hook meant for the main session (a
 * Stop hook that runs the tests, a notifier) would otherwise fire once per subagent.
 */
export function subagentProcessEnv(profile: AgentProfile | undefined, base: NodeJS.ProcessEnv = process.env, identity?: Identity): NodeJS.ProcessEnv {
  const hooks = base[HOOKS_ENV];
  const inherited = { ...base };
  // User settings survive; run identity, trust delegation and browser mode belong to this child.
  for (const key of [HOOKS_ENV, TRUST_ROOT_ENV, BROWSER_VISIBLE_ENV, AGENT_ID_ENV, PARENT_ID_ENV, AGENT_TASK_ENV, AGENT_COLLABORATE_ENV, AGENT_SCRATCH_ENV]) delete inherited[key];
  return {
    ...inherited,
    ...(profile?.hooks && hooks !== undefined ? { [HOOKS_ENV]: hooks } : {}),
    [SUBAGENT_ENV]: '1',
    ...(profile?.visibleBrowser ? { [BROWSER_VISIBLE_ENV]: '1' } : {}),
    ...(profile?.mcp === false ? { [MCP_ENV]: '0' } : {}),
    ...(identity ? { [AGENT_ID_ENV]: identity.id } : {}),
    ...(identity?.parentId ? { [PARENT_ID_ENV]: identity.parentId } : {}),
    ...(identity?.task ? { [AGENT_TASK_ENV]: identity.task } : {}),
    ...(identity?.collaborate ? { [AGENT_COLLABORATE_ENV]: '1' } : {}),
    ...(identity?.workspace ? { [TEAM_WORKSPACE_ENV]: identity.workspace } : {}),
    ...(identity?.trustRoot ? { [TRUST_ROOT_ENV]: identity.trustRoot } : {}),
    ...(identity?.scratch ? { [AGENT_SCRATCH_ENV]: identity.scratch } : {}),
  };
}

/** The Pi CLI that runs this process, else the installed Pi package's CLI, else `pi` on PATH. */
function piInvocation(argv = process.argv): { command: string; prefix: string[] } {
  const script = argv[1];
  if (script && /(^|[\\/])(pi|cli\.js)$/.test(script) && fs.existsSync(script)) return { command: process.execPath, prefix: [script] };
  try {
    const main = fileURLToPath(import.meta.resolve('@earendil-works/pi-coding-agent'));
    const cli = path.join(path.dirname(main), 'bundle', 'cli.js');
    if (fs.existsSync(cli)) return { command: process.execPath, prefix: [cli] };
  } catch {
    // Fall through to PATH lookup.
  }
  return { command: 'pi', prefix: [] };
}

/** This extension's entry file, so children load the same Octocode tools the parent has. */
function selfExtensionPath(): string | undefined {
  // The entry (`index.js` in dist, `index.ts` in src) sits in the folder above this module's own.
  const dir = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
  return ['index.js', 'index.ts'].map((file) => path.join(dir, file)).find((file) => fs.existsSync(file));
}

export function buildAgentArgs(task: string, profile: AgentProfile | undefined, model: string | undefined, extension = selfExtensionPath()): string[] {
  const args = ['--mode', 'json', '--no-session'];
  // `--no-extensions` also turns off Pi's built-in MCP and tool-search extensions, which serve Octocode's research tools
  // (its GitHub and package registry tools are deferred behind `tool_search`): load them back unless the profile opts out of MCP.
  if (extension) args.push('--no-extensions', '-e', extension, ...(profile?.mcp === false ? [] : ['-e', 'builtin:mcp', '-e', 'builtin:tool-search']));
  const chosenModel = model ?? profile?.model;
  if (chosenModel) args.push('--model', chosenModel);
  if (profile?.tools) args.push('--tools', profile.tools);
  if (profile?.excludeTools) args.push('--exclude-tools', expandExcludes(profile.excludeTools));
  if (profile?.prompt) args.push('--append-system-prompt', profile.prompt);
  // `--` stops Pi's flag parsing, so a task starting with `-` stays the message; Pi still reads a positional `@path`
  // after `--` as a file to attach, so a leading `@` gets a space in front.
  args.push('--', task.startsWith('@') ? ` ${task}` : task);
  return args;
}

/** Excluding `file` makes a profile read-only, so Pi's own edit and write must go too. */
function expandExcludes(excludeTools: string): string {
  const names = excludeTools.split(',').map((name) => name.trim()).filter(Boolean);
  return [...new Set(names.includes('file') ? [...names, 'edit', 'write'] : names)].join(',');
}

/** How long a cancelled subagent gets to exit after SIGTERM before it is killed. */
const KILL_GRACE_MS = 5_000;
/** JSON events can contain images; a malformed child must not grow an unterminated frame forever. */
export const MAX_EVENT_BYTES = 16 * 1024 * 1024;

/** Minutes a child may go without emitting any output while its model is generating (default 10; 0 disables). */
export const SUBAGENT_IDLE_ENV = 'OCTOCODE_SUBAGENT_IDLE_MINUTES';
/** Optional wall-clock limit in minutes for one subagent run (default 0: none). */
export const SUBAGENT_MAX_ENV = 'OCTOCODE_SUBAGENT_MAX_MINUTES';
const DEFAULT_IDLE_MINUTES = 10;
/**
 * Extra quiet time allowed while a tool runs or the run is settling after its answer: a foreground bash command and the
 * hold on background jobs may each stay silent for up to 15 minutes without the child being hung.
 */
const BUSY_GRACE_MS = 15 * 60_000;

export interface WatchdogLimits {
  /** Quiet time before a generating child counts as hung (Infinity: never). */
  idleMs: number;
  /** Total run time before the child is stopped (Infinity: never). */
  maxMs: number;
}

export function watchdogLimits(env: NodeJS.ProcessEnv = process.env): WatchdogLimits {
  const minutes = (name: string, fallback: number) => {
    const value = envInt(env, name, fallback, { min: 0 });
    return value === 0 ? Number.POSITIVE_INFINITY : value * 60_000;
  };
  return { idleMs: minutes(SUBAGENT_IDLE_ENV, DEFAULT_IDLE_MINUTES), maxMs: minutes(SUBAGENT_MAX_ENV, 0) };
}

/** Why a child ended without a usable answer: its exit code, or the signal that killed it. */
export function exitReason(code: number | null, signal: NodeJS.Signals | null): string {
  return code === null && signal ? `was killed by ${signal}` : `exited with code ${code ?? 'unknown'}`;
}

interface SubagentOutcome {
  text: string;
  usage: Usage;
  /** Set when the child's model call failed (stopReason error/aborted). */
  error?: string;
}

export function runSubagent(
  args: string[],
  cwd: string,
  profile: AgentProfile | undefined,
  identity: Identity,
  signal: AbortSignal | undefined,
  onProgress: (activity: string | undefined, usage: Usage, image?: { data: string; mimeType: string }) => void,
  onSpawn?: (pid: number) => void,
  limits: WatchdogLimits = watchdogLimits(),
): Promise<SubagentOutcome> {
  if (signal?.aborted) return Promise.reject(new Error('Subagent cancelled'));
  const { command, prefix } = piInvocation();
  return new Promise((resolve, reject) => {
    // Detached on POSIX so the child leads a process group: cancelling kills its whole tree (MCP servers, Chrome, bash).
    const child = spawn(command, [...prefix, ...args], { cwd, env: subagentProcessEnv(profile, process.env, identity), stdio: ['ignore', 'pipe', 'pipe'], detached: process.platform !== 'win32' });
    const usage = emptyUsage();
    const report = new ReportTracker(identity.scratch);
    let buffer = '';
    let stderr = '';
    let frameBytes = 0;
    let streamFailure: Error | undefined;
    const abort = () => killTree(child.pid, KILL_GRACE_MS);
    signal?.addEventListener('abort', abort, { once: true });
    // Watchdog: a child stuck on a stalled model stream emits nothing; stop it instead of spinning forever.
    const startedAt = Date.now();
    let activeAt = startedAt;
    let lastActivity = 'started';
    let toolsRunning = 0;
    let settling = false;
    let stalled: Error | undefined;
    const watchdog = Number.isFinite(limits.idleMs) || Number.isFinite(limits.maxMs)
      ? setInterval(() => {
        if (streamFailure || stalled) return;
        const now = Date.now();
        const quietLimit = limits.idleMs + (toolsRunning > 0 || settling ? BUSY_GRACE_MS : 0);
        const last = `last activity: ${lastActivity}, ${formatDuration(now - activeAt)} ago`;
        if (now - activeAt >= quietLimit) stalled = new Error(`Subagent stopped: no output for ${formatDuration(now - activeAt)}; ${last}. Its process tree was killed.`);
        else if (now - startedAt >= limits.maxMs) stalled = new Error(`Subagent stopped: it ran past its ${formatDuration(limits.maxMs)} limit (${SUBAGENT_MAX_ENV}); ${last}. Its process tree was killed.`);
        else return;
        abort();
      }, Math.max(250, Math.min(30_000, limits.idleMs / 4, limits.maxMs / 4)))
      : undefined;
    watchdog?.unref();
    // Progress is display only: a failure there (a screenshot write, a stale render) must not throw out of the stream handler.
    const progress: typeof onProgress = (...update) => {
      try {
        onProgress(...update);
      } catch {
        // Keep the run going; the final answer does not depend on it.
      }
    };
    const handleLine = (line: string) => {
      const event = parseEvent(line);
      if (!event) return;
      // Calls a tool makes itself (a codemode script) are part of that call, not activity of their own.
      if (event['parentToolCallId']) return;
      if (event['type'] === 'tool_execution_start') {
        toolsRunning++;
        settling = false;
        lastActivity = describeToolCall(event['toolName'], event['args']);
        progress(lastActivity, usage);
      }
      if (event['type'] === 'tool_execution_end') {
        toolsRunning = Math.max(0, toolsRunning - 1);
        const image = imagesOf(event['result']).at(-1);
        if (image) progress(undefined, usage, image);
      }
      if (event['type'] !== 'message_end' || !isRecord(event['message'])) return;
      const message = event['message'];
      if (message['role'] !== 'assistant') {
        report.incoming(message['role']);
        return;
      }
      addUsage(usage, message['usage']);
      progress(undefined, usage);
      settling = message['stopReason'] !== 'toolUse';
      lastActivity = settling ? 'answered; settling' : 'model turn';
      report.answer(assistantText(message), message['stopReason'], message['errorMessage']);
    };
    child.stdout.setEncoding('utf8').on('data', (chunk: string) => {
      if (streamFailure) return;
      activeAt = Date.now();
      let start = 0;
      while (start < chunk.length) {
        const newline = chunk.indexOf('\n', start);
        const part = chunk.slice(start, newline < 0 ? undefined : newline);
        frameBytes += Buffer.byteLength(part);
        if (frameBytes > MAX_EVENT_BYTES) {
          streamFailure = new Error(`Subagent JSON event exceeds ${MAX_EVENT_BYTES} bytes`);
          buffer = '';
          abort();
          return;
        }
        buffer += part;
        if (newline < 0) break;
        handleLine(buffer);
        buffer = '';
        frameBytes = 0;
        start = newline + 1;
      }
    });
    child.stderr.setEncoding('utf8').on('data', (chunk: string) => {
      stderr = (stderr + chunk).slice(-4000);
    });
    child.on('error', (error) => {
      clearInterval(watchdog);
      signal?.removeEventListener('abort', abort);
      reject(error);
    });
    child.on('close', (code, exitSignal) => {
      clearInterval(watchdog);
      if (buffer.trim()) handleLine(buffer);
      signal?.removeEventListener('abort', abort);
      const { text: finalText, error, answered } = report.result();
      if (streamFailure) reject(streamFailure);
      // A child stopped while settling after its answer still delivers that answer.
      else if (stalled && !answered) reject(stalled);
      else if (signal?.aborted) reject(new Error('Subagent cancelled'));
      // The child's report and stderr are untrusted text headed for the model and the terminal. A crash before any
      // final answer is a failure even when the child narrated progress first: narration is not a report.
      else if (code !== 0 && !answered) {
        const last = finalText ? `\nLast progress: ${sanitizeTerminalText(finalText).trim().slice(0, 500)}` : '';
        reject(new Error(`Subagent ${exitReason(code, exitSignal)}: ${sanitizeTerminalText(stderr).trim() || 'no output'}${last}`));
      }
      else resolve({ text: sanitizeTerminalText(finalText), usage, ...(error ? { error: sanitizeTerminalText(error) } : {}) });
    });
    // Install cleanup before invoking a callback that can fail (for example, recording worktree ownership).
    try {
      if (child.pid !== undefined) onSpawn?.(child.pid);
    } catch (error) {
      streamFailure = error instanceof Error ? error : new Error(String(error));
      abort();
    }
  });
}

function parseEvent(line: string): Record<string, unknown> | undefined {
  try {
    const value = JSON.parse(line) as unknown;
    return isRecord(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

export function assistantText(message: unknown): string | undefined {
  if (!isRecord(message) || message['role'] !== 'assistant' || !Array.isArray(message['content'])) return undefined;
  const text = message['content']
    .filter((part): part is { type: 'text'; text: string } => isRecord(part) && part['type'] === 'text' && typeof part['text'] === 'string')
    .map((part) => part.text)
    .join('\n')
    .trim();
  return text || undefined;
}

function describeToolCall(name: unknown, args: unknown): string {
  const hint = toolHint(args);
  return `→ ${String(name)}${hint ? ` ${hint}` : ''}`;
}
