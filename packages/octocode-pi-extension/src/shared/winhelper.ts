import { spawn, type ChildProcess, type SpawnOptions } from 'node:child_process';
import { classifyScript, type Classification, type ScriptCommand } from './winclassify.js';
import { WINDOWS_HELPER_SCRIPT } from './winhelper-script.js';
import { isRecord, parseJson, validPid } from './util.js';
import { powerShellCandidates } from './winpath.js';

/**
 * The Windows helper: one `pwsh` (else Windows PowerShell), started by absolute path, per Pi process, started on first use, that owns a Job Object per
 * process tree and parses PowerShell for the shell classifier (the program is winhelper-script.ts). It exits when its
 * stdin closes, which ends every tree assigned to it, so the trees die with Pi even when Pi crashes.
 *
 * It fails closed: with no usable shell, a crash, a timeout or a parse error, a call answers "no" (`assign`/`kill`
 * false, `parse` undefined, `classifyPowerShell` with `parsed: false`), never a made-up success. Known gaps: a
 * grandchild started before the assignment is not in the job, nor is one made through `Win32_Process.Create`.
 */

/** How long the helper gets to print its ready line (it compiles C# first; Windows PowerShell can take seconds). */
const STARTUP_MS = 30_000;
/** How long one request gets before it counts as failed. */
const REQUEST_MS = 15_000;
/** How long a stopping helper gets to exit after its stdin closed, before it is killed. */
const STOP_MS = 3_000;

const SPAWN_OPTIONS: SpawnOptions = { stdio: ['pipe', 'pipe', 'ignore'], windowsHide: true };

export interface HelperOptions {
  /** `child_process.spawn`, replaceable. */
  spawn?: (command: string, args: string[], options: SpawnOptions) => ChildProcess;
  /** The platform process trees are managed on; anything but `win32` has no job to assign. */
  platform?: () => NodeJS.Platform;
  /** PowerShell executables to try, in order; by default `pwsh` from PATH, then System32's Windows PowerShell, by absolute path (see winpath.ts). */
  shells?: readonly string[];
  startupMs?: number;
  requestMs?: number;
  stopMs?: number;
}

interface Response {
  id?: number;
  ready?: boolean;
  ok?: boolean;
  error?: string;
  commands?: unknown;
}

/** The parsed JSON object on this line, or undefined for anything else (PowerShell may print stray text). */
function parseLine(line: string): Response | undefined {
  const value = parseJson(line);
  return isRecord(value) ? (value as Response) : undefined;
}

/** The helper's `commands` when they have the shape the classifier reads. */
function scriptCommands(value: unknown): ScriptCommand[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const valid = value.every((each) => typeof each === 'object' && each !== null && Array.isArray((each as ScriptCommand).args) && ((each as ScriptCommand).name === null || typeof (each as ScriptCommand).name === 'string'));
  return valid ? (value as ScriptCommand[]) : undefined;
}

export class WindowsHelper {
  readonly #options: HelperOptions;
  #child: ChildProcess | undefined;
  #starting: Promise<boolean> | undefined;
  #unavailable = false;
  #next = 1;
  readonly #pending = new Map<number, (response: Response | undefined) => void>();
  /** Roots with a job: assigned and not yet killed. */
  readonly #jobs = new Set<number>();
  readonly #assigning = new Map<number, Promise<boolean>>();

  constructor(options: HelperOptions = {}) {
    this.#options = options;
  }

  #platform(): NodeJS.Platform {
    return this.#options.platform ? this.#options.platform() : process.platform;
  }

  /**
   * Starts the helper unless it runs; true when it does. Call it before spawning a process whose tree is to be tracked,
   * so the first assignment does not wait for the startup. False (and never retried until `stopAll`) when no PowerShell
   * starts.
   */
  start(): Promise<boolean> {
    if (this.#child) return Promise.resolve(true);
    if (this.#unavailable) return Promise.resolve(false);
    this.#starting ??= this.#launch().finally(() => {
      this.#starting = undefined;
    });
    return this.#starting;
  }

  async #launch(): Promise<boolean> {
    for (const shell of this.#options.shells ?? powerShellCandidates()) if (await this.#open(shell)) return true;
    this.#unavailable = true;
    return false;
  }

  /** Starts `shell` as the helper; true once it printed its ready line, false when it could not start or died first. */
  #open(shell: string): Promise<boolean> {
    const encoded = Buffer.from(WINDOWS_HELPER_SCRIPT, 'utf16le').toString('base64');
    return new Promise((resolve) => {
      let child: ChildProcess;
      try {
        child = (this.#options.spawn ?? spawn)(shell, ['-NoLogo', '-NoProfile', '-NonInteractive', '-EncodedCommand', encoded], SPAWN_OPTIONS);
      } catch {
        resolve(false);
        return;
      }
      const { stdin, stdout } = child;
      if (!stdin || !stdout) {
        child.kill();
        resolve(false);
        return;
      }
      let ready = false;
      let gone = false;
      let buffer = '';
      const startup = setTimeout(() => child.kill(), this.#options.startupMs ?? STARTUP_MS);
      stdin.on('error', () => undefined);
      stdout.on('error', () => undefined);
      stdout.setEncoding('utf8');
      stdout.on('data', (chunk: string) => {
        buffer += chunk;
        for (let end = buffer.indexOf('\n'); end >= 0; end = buffer.indexOf('\n')) {
          const message = parseLine(buffer.slice(0, end).trim());
          buffer = buffer.slice(end + 1);
          if (!message) continue;
          if (!ready && message.ready === true) {
            ready = true;
            clearTimeout(startup);
            this.#child = child;
            // The helper must not keep Pi's process alive; a request in flight does, through its timer.
            child.unref();
            (stdin as unknown as { unref?: () => void }).unref?.();
            (stdout as unknown as { unref?: () => void }).unref?.();
            resolve(true);
          } else if (typeof message.id === 'number') this.#pending.get(message.id)?.(message);
        }
      });
      const exited = () => {
        if (gone) return;
        gone = true;
        clearTimeout(startup);
        // Its jobs closed with it, so their processes are gone too; a newer helper is not ours to reset.
        if (this.#child === child) {
          this.#child = undefined;
          this.#jobs.clear();
          this.#failPending();
        }
        if (!ready) resolve(false);
      };
      child.once('error', exited);
      child.once('exit', exited);
    });
  }

  #failPending(): void {
    for (const settle of [...this.#pending.values()]) settle(undefined);
  }

  /** One request and its response; undefined when the helper is unavailable, died, did not answer in time or the write failed. */
  async #call(request: Record<string, unknown>): Promise<Response | undefined> {
    if (!(await this.start())) return undefined;
    const child = this.#child;
    if (!child?.stdin) return undefined;
    const id = this.#next++;
    return new Promise((resolve) => {
      const settle = (response: Response | undefined) => {
        clearTimeout(timer);
        this.#pending.delete(id);
        resolve(response);
      };
      const timer = setTimeout(() => settle(undefined), this.#options.requestMs ?? REQUEST_MS);
      this.#pending.set(id, settle);
      child.stdin!.write(`${JSON.stringify({ id, ...request })}\n`, (error) => {
        if (error) settle(undefined);
      });
    });
  }

  /**
   * Puts the process `pid` (just spawned) in a new job whose processes all end when it is killed or the helper exits.
   * True once assigned; false off Windows, without a helper, or when Windows refuses (it already exited).
   */
  assign(pid: number | undefined): Promise<boolean> {
    if (this.#platform() !== 'win32' || !validPid(pid)) return Promise.resolve(false);
    const assigning = this.#call({ cmd: 'assign', pid }).then((response) => {
      this.#assigning.delete(pid);
      const assigned = response?.ok === true;
      if (assigned) this.#jobs.add(pid);
      return assigned;
    });
    this.#assigning.set(pid, assigning);
    return assigning;
  }

  /** Whether `pid` has a job, or is being assigned one: a kill by `kill` rather than by taskkill. */
  owns(pid: number | undefined): boolean {
    return validPid(pid) && (this.#jobs.has(pid) || this.#assigning.has(pid));
  }

  /** Ends every process in `pid`'s job, the root's descendants included, also after the root exited. False when it has none or the helper failed. */
  async kill(pid: number | undefined): Promise<boolean> {
    if (!validPid(pid)) return false;
    await this.#assigning.get(pid);
    if (!this.#jobs.delete(pid)) return false;
    return (await this.#call({ cmd: 'kill', pid }))?.ok === true;
  }

  /** The commands of a PowerShell script as the parser sees them; undefined when it does not parse or there is no helper. */
  async parse(script: string): Promise<ScriptCommand[] | undefined> {
    const response = await this.#call({ cmd: 'parse', command: script });
    return response?.ok === true ? scriptCommands(response.commands) : undefined;
  }

  /** Ends the helper, and with it every tree assigned to it. A later call starts a new one. */
  async stopAll(): Promise<void> {
    await this.#starting;
    const child = this.#child;
    this.#unavailable = false;
    if (!child) return;
    this.#child = undefined;
    this.#jobs.clear();
    this.#failPending();
    const exited = new Promise<void>((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) resolve();
      else child.once('exit', () => resolve());
    });
    const timer = setTimeout(() => child.kill(), this.#options.stopMs ?? STOP_MS);
    child.stdin?.end();
    await exited;
    clearTimeout(timer);
  }
}

/** Process trees and the script parser of this Pi process. */
export const windowsTree = new WindowsHelper();

/**
 * Whether a PowerShell command is destructive: Remove-Item (or `rm`, `del`, `rd`…) that is recursive or forced on a root,
 * the profile, a system folder or a wildcard, Format-Volume, Clear-Disk, Stop-Computer, cmd's `rd /s` and `del /s`, also
 * inside `pwsh -Command`, `-EncodedCommand` and `Invoke-Expression`. `parsed: false` when no verdict could be reached
 * (no PowerShell, a parse error, an unreadable layer): the caller treats that as destructive.
 */
export function classifyPowerShell(command: string, helper: Pick<WindowsHelper, 'parse'> = windowsTree): Promise<Classification> {
  return classifyScript(command, (script) => helper.parse(script));
}
