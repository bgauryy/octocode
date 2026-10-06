import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { SettingsManager, getAgentDir, getShellConfig, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { PRIVATE_FILE_MODE, privateDir, sessionOutputDir } from '../shared/home.js';
import { formatDuration, shortPath } from '../shared/format.js';
import { killTree } from '../shared/process.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { capOutput, errorMessage, firstLine } from '../shared/util.js';

/**
 * Background shell jobs for the `bash` tool (bash.ts): each runs in its own process group, with the user's Pi shell,
 * command prefix and Pi's environment, writing to a size-capped log in the session's `bash/` folder.
 */

/** Background jobs running at once; more are refused. */
export const MAX_BASH_JOBS = 8;
/** The longest timer Node keeps (2^31-1 ms), in whole seconds. */
export const MAX_TIMER_SECONDS = 2_147_483;
/** Output tail a finished job reports back. */
const TAIL_BYTES = 4 * 1024;
const TAIL_LINES = 40;
/** A job log past this size drops its older output (keeping the tail), so a chatty server cannot fill the disk. */
export const MAX_JOB_LOG_BYTES = 16 * 1024 * 1024;
/** How often a running job's log size is checked, in ms. */
const LOG_CHECK_MS = 2_000;
export const JOB_TYPE = 'octocode-bash-job';
/** The user's Pi shell settings that every command runs with. */
export interface ShellSettings {
  shellPath?: string;
  commandPrefix?: string;
}

/** `shellPath` and `shellCommandPrefix` from Pi's settings for `cwd` (project settings only in a trusted project). */
export function shellSettings(cwd: string, projectTrusted: boolean): ShellSettings {
  try {
    const settings = SettingsManager.create(cwd, getAgentDir(), { projectTrusted });
    const shellPath = settings.getShellPath();
    const commandPrefix = settings.getShellCommandPrefix();
    return { ...(shellPath ? { shellPath } : {}), ...(commandPrefix ? { commandPrefix } : {}) };
  } catch {
    return {};
  }
}

/** The command as Pi's bash runs it: the settings prefix on its own line first. */
export function prefixed(command: string, commandPrefix?: string): string {
  return commandPrefix ? `${commandPrefix}\n${command}` : command;
}

/** Keeps a log under `max` bytes: past it, the log restarts with a note and its last part. */
export function capLog(file: string, max = MAX_JOB_LOG_BYTES): void {
  try {
    if (fs.statSync(file).size <= max) return;
    const tail = logTail(file, Math.min(64 * 1024, Math.floor(max / 2)));
    // The job writes with O_APPEND, so after the truncation its next write lands at the new end.
    fs.truncateSync(file, 0);
    fs.appendFileSync(file, `[earlier output dropped: the log passed ${Math.round(max / 1024 / 1024)} MB]\n${tail}`);
  } catch {
    // The log is gone.
  }
}

/**
 * The environment Pi's own bash gives a command (core/tools/bash `resolveSpawnContext` over utils/shell `getShellEnv`,
 * neither exported): Pi's bin folder first on PATH, and the PI_* session variables set from this session, never
 * inherited from the parent. The builtin is created without a spawnHook, so there is none to apply here.
 */
export function shellEnv(ctx?: ExtensionContext, base: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  const bin = path.join(getAgentDir(), 'bin');
  const pathKey = Object.keys(base).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
  const current = base[pathKey] ?? '';
  const env: NodeJS.ProcessEnv = { ...base, [pathKey]: current.split(path.delimiter).filter(Boolean).includes(bin) ? current : [bin, current].filter(Boolean).join(path.delimiter) };
  for (const name of ['PI_SESSION_ID', 'PI_SESSION_FILE', 'PI_PROVIDER', 'PI_MODEL', 'PI_REASONING_LEVEL']) delete env[name];
  if (!ctx) return env;
  env['PI_SESSION_ID'] = ctx.sessionManager.getSessionId();
  const sessionFile = ctx.sessionManager.getSessionFile();
  if (sessionFile) env['PI_SESSION_FILE'] = sessionFile;
  if (ctx.model) {
    env['PI_PROVIDER'] = ctx.model.provider;
    env['PI_MODEL'] = ctx.model.id;
  }
  if (ctx.thinkingLevel) env['PI_REASONING_LEVEL'] = ctx.thinkingLevel;
  return env;
}

export interface BashJob {
  id: string;
  command: string;
  /** Where it runs; its log path is shown relative to it. */
  cwd: string;
  pid: number;
  log: string;
  startedAt: number;
  /** Seconds after which the job is stopped; unset: it runs until it exits, is stopped, or the session ends. */
  timeout?: number;
  /** Why the extension stopped it, when it did. */
  stopped?: 'timeout' | 'user' | 'session end' | 'run end';
  done: Promise<JobExit>;
}

export interface JobExit {
  code: number | null;
  signal: NodeJS.Signals | null;
  seconds: number;
}

/** The last few KB of a job's log, cut to whole lines. */
export function logTail(file: string, maxBytes = TAIL_BYTES): string {
  try {
    const fd = fs.openSync(file, 'r');
    try {
      const size = fs.fstatSync(fd).size;
      const length = Math.min(size, maxBytes);
      const buffer = Buffer.alloc(length);
      fs.readSync(fd, buffer, 0, length, size - length);
      const text = buffer.toString('utf8');
      return size > length ? text.slice(text.indexOf('\n') + 1) : text;
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return '';
  }
}

/** Background shell jobs of one session: each runs in its own process group with output going to a log file. */
export class BashJobs {
  readonly jobs = new Map<string, BashJob>();
  private next = 0;

  constructor(private readonly dir?: string) {}

  /** Where logs go: the given directory, else the current session's `bash/` folder. */
  logDir(): string {
    return this.dir ?? sessionOutputDir('bash');
  }

  async start(
    command: string,
    cwd: string,
    asked: number | undefined,
    onExit: (job: BashJob, exit: JobExit) => void,
    { env = shellEnv(), shell: settings = {}, maxLogBytes = MAX_JOB_LOG_BYTES }: { env?: NodeJS.ProcessEnv; shell?: ShellSettings; maxLogBytes?: number } = {},
  ): Promise<BashJob> {
    if (this.jobs.size >= MAX_BASH_JOBS) throw new Error(`${this.jobs.size} background jobs are already running (${[...this.jobs.keys()].join(', ')}); the limit is ${MAX_BASH_JOBS}. Stop one first.`);
    if (!fs.existsSync(cwd)) throw new Error(`Working directory does not exist: ${cwd}`);
    // Node fires a timer longer than 2^31-1 ms at once, which would stop the job as soon as it started.
    const timeout = asked && asked > 0 ? Math.min(asked, MAX_TIMER_SECONDS) : undefined;
    const dir = this.logDir();
    privateDir(dir);
    const id = `bash-${++this.next}`;
    // Job ids restart with each extension instance (a /reload), so the start time keeps an earlier log.
    const log = path.join(dir, `${id}-${Date.now()}.log`);
    // Append mode, so capLog can truncate the file under the running job.
    const out = fs.openSync(log, 'a', PRIVATE_FILE_MODE);
    const shell = getShellConfig(settings.shellPath);
    const viaStdin = shell.commandTransport === 'stdin';
    const script = prefixed(command, settings.commandPrefix);
    let child;
    try {
      child = spawn(shell.shell, viaStdin ? shell.args : [...shell.args, script], {
        cwd,
        env,
        detached: process.platform !== 'win32',
        stdio: [viaStdin ? 'pipe' : 'ignore', out, out],
        windowsHide: true,
      });
    } finally {
      fs.closeSync(out);
    }
    if (child.pid === undefined) {
      // Spawning failed (no shell, EAGAIN, EMFILE): the 'error' event that follows names the cause. Nothing ran, so
      // the empty log goes too.
      const cause = await new Promise<unknown>((resolve) => child.once('error', resolve));
      fs.rmSync(log, { force: true });
      throw new Error(`Could not start a background shell (${shell.shell}): ${errorMessage(cause)}. Nothing is running.`);
    }
    if (viaStdin) {
      child.stdin?.on('error', () => undefined);
      child.stdin?.end(script);
    }
    // The job must not keep a print run's process alive; session_shutdown stops it.
    child.unref();
    const startedAt = Date.now();
    let timer: NodeJS.Timeout | undefined;
    const sizeCheck = setInterval(() => capLog(log, maxLogBytes), LOG_CHECK_MS);
    sizeCheck.unref();
    const done = new Promise<JobExit>((resolve) => {
      const finish = (code: number | null, signal: NodeJS.Signals | null) => {
        if (timer) clearTimeout(timer);
        clearInterval(sizeCheck);
        capLog(log, maxLogBytes);
        resolve({ code, signal, seconds: Math.round((Date.now() - startedAt) / 1000) });
      };
      child.once('error', (error) => {
        fs.appendFileSync(log, `\n${errorMessage(error)}\n`);
        finish(null, null);
      });
      child.once('exit', finish);
    });
    const job: BashJob = { id, command, cwd, pid: child.pid, log, startedAt, ...(timeout ? { timeout } : {}), done };
    if (timeout) {
      timer = setTimeout(() => this.stop(id, 'timeout'), timeout * 1000);
      timer.unref();
    }
    this.jobs.set(id, job);
    void done.then((exit) => {
      this.jobs.delete(id);
      onExit(job, exit);
    });
    return job;
  }

  /** Stops a job's whole process tree; false when no such job runs. */
  stop(id: string, reason: NonNullable<BashJob['stopped']>): boolean {
    const job = this.jobs.get(id);
    if (!job) return false;
    job.stopped ??= reason;
    killTree(job.pid);
    return true;
  }

  stopAll(reason: NonNullable<BashJob['stopped']>): void {
    for (const id of [...this.jobs.keys()]) this.stop(id, reason);
  }

  /**
   * Before a headless run ends: jobs without a timeout (a server, a watcher) would never finish, so they are stopped;
   * resolves once every job has exited (each exit sends its report).
   */
  settle(): Promise<unknown> {
    for (const job of [...this.jobs.values()]) if (!job.timeout) this.stop(job.id, 'run end');
    return Promise.all([...this.jobs.values()].map((job) => job.done));
  }

  /** One line per running job, for `/octocode jobs`. */
  describe(now = Date.now()): string {
    if (this.jobs.size === 0) return 'No background bash jobs are running.';
    return [...this.jobs.values()]
      .map((job) => `${job.id} · ${formatDuration(now - job.startedAt)}${job.timeout ? ` of ${formatDuration(job.timeout * 1000)}` : ''} · pid ${job.pid} · ${firstLine(job.command).slice(0, 80)}\n  log: ${shortPath(job.log)}`)
      .join('\n');
  }
}

/** The message a finished job sends: how it ended, how long it ran, and the tail of its output. */
export function jobReport(job: BashJob, exit: JobExit): string {
  const how =
    job.stopped === 'timeout'
      ? `stopped after its ${formatDuration((job.timeout ?? 0) * 1000)} timeout`
      : job.stopped
        ? `stopped (${job.stopped})`
        : exit.code === 0
          ? 'finished'
          : exit.code !== null
            ? `failed with exit code ${exit.code}`
            : `ended by ${exit.signal ?? 'an error'}`;
  // Logs hold raw terminal output: colors and cursor moves would only cost tokens (and could hide text).
  const tail = sanitizeTerminalText(logTail(job.log)).trimEnd();
  return [`Background bash ${job.id} ${how} after ${formatDuration(exit.seconds * 1000)}: ${firstLine(job.command).slice(0, 120)}`, `Log: ${shortPath(job.log, job.cwd)}`, tail ? capOutput(tail, TAIL_BYTES, TAIL_LINES) : '(no output)'].join('\n');
}

