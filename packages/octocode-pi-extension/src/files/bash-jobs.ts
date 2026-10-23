import { spawn } from 'node:child_process';
import fs from 'node:fs';
import type { Socket } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { SettingsManager, getAgentDir, getShellConfig, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { atomicWriteFileSync } from '../shared/atomic.js';
import { PRIVATE_FILE_MODE, privateDir, sessionOutputDir } from '../shared/home.js';
import { formatDuration, shortPath } from '../shared/format.js';
import { adoptTree, groupAlive, killTree, prepareTree, processStartTime, stopTree } from '../shared/process.js';
import { RotatingLog } from '../shared/rotating-log.js';
import { errorMessage, firstLine, wholeSeconds } from '../shared/util.js';
import { recordFile, sessionBashDirs, type JobRecord } from './bash-orphans.js';

/**
 * Background shell jobs for the `bash` tool (bash.ts): each runs in its own process group, with the user's Pi shell,
 * command prefix and Pi's environment. Its output comes back through pipes and Pi writes it to a size-capped,
 * rotating log in the session's `bash/` folder. Next to each log an owner record names the job and the Pi that started
 * it (bash-orphans.ts); its report when it ends is bash-report.ts.
 */

/** The longest timer Node keeps (2^31-1 ms), in whole seconds. */
export const MAX_TIMER_SECONDS = 2_147_483;
/** A job log past this size moves to `<log>.1` (replacing the one before) and starts over, so a chatty server cannot fill the disk. */
const MAX_JOB_LOG_BYTES = 16 * 1024 * 1024;
/** After the job's process group is gone, how long its pipes may take to deliver the last output, in ms. */
const DRAIN_MS = 1_000;
/** How often a job whose shell exited checks whether children it left in its group are still running, in ms. */
const GROUP_POLL_MS = 250;
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
  stopped?: 'timeout' | 'user' | 'session end' | 'reload' | 'run end';
  /** Its process group is gone; only its last output may still be draining before `done` resolves. */
  exited?: boolean;
  done: Promise<JobExit>;
}


export interface JobExit {
  code: number | null;
  signal: NodeJS.Signals | null;
  seconds: number;
  /** Set when the job's log stopped recording: its size then and why. */
  logStopped?: { bytes: number; error: string };
  /** Set when the log rotated: how many times (the second rotation and later drop the oldest output). */
  logRotations?: number;
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
    if (!fs.existsSync(cwd)) throw new Error(`Working directory does not exist: ${cwd}`);
    // Node fires a timer longer than 2^31-1 ms at once, which would stop the job as soon as it started.
    const timeout = asked && asked > 0 ? Math.min(asked, MAX_TIMER_SECONDS) : undefined;
    const dir = this.logDir();
    privateDir(dir);
    const id = `bash-${++this.next}`;
    // Job ids restart with each extension instance (a /reload), so the start time keeps an earlier log.
    const log = path.join(dir, `${id}-${Date.now()}.log`);
    const output = new RotatingLog(log, maxLogBytes);
    const shell = getShellConfig(settings.shellPath);
    const viaStdin = shell.commandTransport === 'stdin';
    const script = prefixed(command, settings.commandPrefix);
    // On Windows the job that holds the tree exists before the shell does.
    await prepareTree();
    const child = spawn(shell.shell, viaStdin ? shell.args : [...shell.args, script], {
      cwd,
      env,
      detached: process.platform !== 'win32',
      stdio: [viaStdin ? 'pipe' : 'ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });
    if (child.pid === undefined) {
      // Spawning failed (no shell, EAGAIN, EMFILE): the 'error' event that follows names the cause. Nothing ran, so
      // the empty log goes too.
      const cause = await new Promise<unknown>((resolve) => child.once('error', resolve));
      output.close();
      fs.rmSync(log, { force: true });
      throw new Error(`Could not start a background shell (${shell.shell}): ${errorMessage(cause)}. Nothing is running.`);
    }
    adoptTree(child.pid);
    if (viaStdin) {
      child.stdin?.on('error', () => undefined);
      child.stdin?.end(script);
    }
    const record = recordFile(log);
    try {
      const owned: JobRecord = { id, pid: child.pid, started: processStartTime(child.pid), command, log, host: os.hostname(), owner: { pid: process.pid, started: processStartTime(process.pid) } };
      atomicWriteFileSync(record, JSON.stringify(owned), { mode: PRIVATE_FILE_MODE });
    } catch {
      // Without its record only this Pi can stop the job.
    }
    // The job, and its output pipes, must not keep a print run's process alive; session_shutdown stops it.
    child.unref();
    const pipes = [child.stdout, child.stderr].filter((pipe) => pipe !== null) as Socket[];
    const drained = Promise.all(pipes.map((pipe) => new Promise<void>((resolve) => pipe.once('close', () => resolve()))));
    for (const pipe of pipes) {
      pipe.on('data', (chunk: Buffer) => output.write(chunk));
      pipe.on('error', () => undefined);
      pipe.unref();
    }
    const startedAt = Date.now();
    let timer: NodeJS.Timeout | undefined;
    // The job, for `finish` to mark exited: it exists before any child event can fire.
    const state: { job?: BashJob } = {};
    const done = new Promise<JobExit>((resolve) => {
      let finished = false;
      const finish = (code: number | null, signal: NodeJS.Signals | null) => {
        if (finished) return;
        finished = true;
        if (state.job) state.job.exited = true;
        if (timer) clearTimeout(timer);
        // The pipes close once every writer is gone; a process that left the group may hold them open forever.
        void Promise.race([drained, new Promise((settle) => setTimeout(settle, DRAIN_MS).unref())]).then(() => {
          for (const pipe of pipes) pipe.destroy();
          output.close();
          const seconds = wholeSeconds(Date.now() - startedAt);
          resolve({ code, signal, seconds, ...(output.failure ? { logStopped: output.failure } : {}), ...(output.rotations > 0 ? { logRotations: output.rotations } : {}) });
        });
      };
      child.once('error', (error) => {
        output.write(`\n${errorMessage(error)}\n`);
        finish(null, null);
      });
      // The shell can exit while children it started (`server &`) still run in its group and write to the log: the
      // job lasts until the group is gone, so they stay capped, stoppable and stopped at session end.
      child.once('exit', (code, signal) => {
        const wait = (): void => {
          if (groupAlive(child.pid)) setTimeout(wait, GROUP_POLL_MS).unref();
          else finish(code, signal);
        };
        wait();
      });
    });
    const job: BashJob = { id, command, cwd, pid: child.pid, log, startedAt, ...(timeout ? { timeout } : {}), done };
    state.job = job;
    if (timeout) {
      timer = setTimeout(() => this.stop(id, 'timeout'), timeout * 1000);
      timer.unref();
    }
    this.jobs.set(id, job);
    void done.then((exit) => {
      this.jobs.delete(id);
      try {
        fs.rmSync(record, { force: true });
      } catch {
        // A later Pi finds the job gone and removes the record.
      }
      onExit(job, exit);
    });
    return job;
  }

  /** Stops a job's whole process tree; false when no such job runs. A job that already exited keeps its own status. */
  stop(id: string, reason: NonNullable<BashJob['stopped']>): boolean {
    const job = this.jobs.get(id);
    if (!job) return false;
    if (job.exited) return true;
    job.stopped ??= reason;
    killTree(job.pid);
    return true;
  }

  /** Jobs whose process group still runs (an exited job only waits for its last output). */
  running(): BashJob[] {
    return [...this.jobs.values()].filter((job) => !job.exited);
  }

  /** Stops every running job; resolves once each tree that outlived SIGTERM got SIGKILL (Pi may exit right after). */
  async stopAll(reason: NonNullable<BashJob['stopped']>): Promise<void> {
    const jobs = this.running();
    for (const job of jobs) job.stopped ??= reason;
    await Promise.all(jobs.map((job) => stopTree(job.pid)));
  }

  /**
   * Before a headless run ends: jobs without a timeout (a server, a watcher) would never finish, so they are stopped;
   * resolves once every job has exited (each exit sends its report).
   */
  settle(): Promise<unknown> {
    for (const job of this.running()) if (!job.timeout) this.stop(job.id, 'run end');
    return Promise.all([...this.jobs.values()].map((job) => job.done));
  }

  /** The owner-record folders to check for jobs left by a dead Pi: the given directory, else every session's. */
  recordDirs(): string[] {
    return this.dir ? [this.dir] : sessionBashDirs();
  }

  /** At a /reload (same process, same session), records the jobs it stops so the next extension instance can name them. */
  noteReload(stopped: readonly BashJob[]): void {
    if (stopped.length === 0) return;
    try {
      atomicWriteFileSync(this.reloadNote(), JSON.stringify(stopped.map(({ id, command, log }) => ({ id, command, log }))), { mode: PRIVATE_FILE_MODE });
    } catch {
      // The reports in the conversation still name them.
    }
  }

  /** The jobs the last /reload of this process stopped (read once). */
  takeReloadNote(): Array<Pick<BashJob, 'id' | 'command' | 'log'>> {
    const file = this.reloadNote();
    try {
      const jobs = JSON.parse(fs.readFileSync(file, 'utf8')) as Array<Pick<BashJob, 'id' | 'command' | 'log'>>;
      return Array.isArray(jobs) ? jobs : [];
    } catch {
      return [];
    } finally {
      fs.rmSync(file, { force: true });
    }
  }

  private reloadNote(): string {
    return path.join(this.logDir(), `reload-${process.pid}.json`);
  }

  /** One line per running job, for `/octocode jobs`. */
  describe(now = Date.now()): string {
    if (this.jobs.size === 0) return 'No background bash jobs are running.';
    return [...this.jobs.values()]
      .map((job) => `${job.id} · ${formatDuration(now - job.startedAt)}${job.timeout ? ` of ${formatDuration(job.timeout * 1000)}` : ''} · pid ${job.pid} · ${firstLine(job.command).slice(0, 80)}\n  log: ${shortPath(job.log)}`)
      .join('\n');
  }
}
