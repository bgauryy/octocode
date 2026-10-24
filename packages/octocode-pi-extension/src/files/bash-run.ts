import { spawn, type ChildProcess } from 'node:child_process';
import fs from 'node:fs';
import { constants } from 'node:os';
import { getShellConfig, type BashOperations } from '@earendil-works/pi-coding-agent';
import { firstLine } from '../shared/util.js';
import { adoptTree, groupAlive, groupMembers, killTree, prepareTree, signalTree } from '../shared/process.js';
import { MAX_TIMER_SECONDS, type ShellSettings } from './bash-jobs.js';

/**
 * How a foreground command runs: Pi's local shell execution (core/tools/bash `createLocalShellOperations`, handed to
 * Pi's bash as its `operations`), with two changes. Esc and the deadline stop the command's tree with SIGTERM and only
 * then SIGKILL (`killTree`), where Pi sends SIGKILL at once and nothing gets to clean up (temp files, lock files, child
 * servers). And the caller learns the command's process group, so it can name processes the command left running
 * (`server &`): Pi stops the group only on a timeout or abort, so they keep running after it returns.
 */

/** After the shell exits, how long its pipes may stay quiet before the output counts as complete (Pi's own value). */
const EXIT_STDIO_GRACE_MS = 100;
/** Processes listed by name in the "left running" note; the rest are counted. */
const LISTED_STRAYS = 5;

/** Process groups of foreground commands running now: Pi kills its own at exit, so these die with Pi too. */
const running = new Set<number>();
const killRunning = (): void => {
  for (const pid of running) signalTree(pid, 'SIGKILL');
};

function track(pid: number): void {
  if (running.size === 0) process.on('exit', killRunning);
  running.add(pid);
}

function untrack(pid: number): void {
  running.delete(pid);
  if (running.size === 0) process.off('exit', killRunning);
}

/**
 * Resolves with the shell's exit code once it exited and its pipes ended, or fell quiet for `EXIT_STDIO_GRACE_MS`: a
 * process it left behind may hold them open forever. Output still arriving keeps it reading (Pi's waitForChildProcess).
 */
function waitForExit(child: ChildProcess): Promise<number | null> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let exited = false;
    let code: number | null = null;
    let idle: NodeJS.Timeout | undefined;
    const open = new Set([child.stdout, child.stderr].filter((pipe) => pipe !== null));
    const finalize = () => {
      if (settled) return;
      settled = true;
      if (idle) clearTimeout(idle);
      child.stdout?.destroy();
      child.stderr?.destroy();
      resolve(code);
    };
    const arm = () => {
      if (idle) clearTimeout(idle);
      idle = setTimeout(finalize, EXIT_STDIO_GRACE_MS);
    };
    for (const pipe of open) {
      pipe.once('end', () => {
        open.delete(pipe);
        if (exited && open.size === 0) finalize();
      });
      pipe.on('data', () => {
        if (exited && !settled) arm();
      });
    }
    child.once('error', (error) => {
      if (settled) return;
      settled = true;
      if (idle) clearTimeout(idle);
      reject(error);
    });
    child.once('exit', (exitCode) => {
      exited = true;
      code = exitCode;
      if (open.size === 0) finalize();
      else arm();
    });
    child.once('close', (exitCode) => {
      code ??= exitCode;
      finalize();
    });
  });
}

/**
 * The foreground shell operations for one call. `onGroup` receives the command's process group leader when the
 * command returned on its own (not on a timeout or abort, which stop the whole group).
 */
export function foregroundOperations(settings: ShellSettings, onGroup: (pid: number) => void): BashOperations {
  return {
    exec: async (command, cwd, { onData, signal, timeout, env }) => {
      if (signal?.aborted) throw new Error('aborted');
      // Node timers stop at MAX_TIMER_SECONDS: a longer timeout is that long, and a non-positive one is no timeout.
      if (timeout !== undefined) timeout = Number.isFinite(timeout) && timeout > 0 ? Math.min(timeout, MAX_TIMER_SECONDS) : undefined;
      if (!fs.existsSync(cwd)) throw new Error(`Working directory does not exist: ${cwd}\nCannot execute bash commands.`);
      const shell = getShellConfig(settings.shellPath);
      const viaStdin = shell.commandTransport === 'stdin';
      // On Windows the job that holds the tree exists before the shell does.
      await prepareTree();
      const child = spawn(shell.shell, viaStdin ? shell.args : [...shell.args, command], {
        cwd,
        detached: process.platform !== 'win32',
        env,
        stdio: [viaStdin ? 'pipe' : 'ignore', 'pipe', 'pipe'],
        windowsHide: true,
      });
      if (viaStdin) {
        child.stdin?.on('error', () => undefined);
        child.stdin?.end(command);
      }
      const pid = child.pid;
      if (pid) {
        track(pid);
        adoptTree(pid);
      }
      let timedOut = false;
      const stop = () => killTree(pid);
      const timer =
        timeout === undefined
          ? undefined
          : setTimeout(() => {
              timedOut = true;
              stop();
            }, timeout * 1000);
      child.stdout?.on('data', onData);
      child.stderr?.on('data', onData);
      if (signal) signal.addEventListener('abort', stop, { once: true });
      try {
        const code = await waitForExit(child);
        if (signal?.aborted) throw new Error('aborted');
        if (timedOut) throw new Error(`timeout:${timeout}`);
        if (pid) onGroup(pid);
        // A signal-killed shell has no exit code: the shell convention keeps it from reading as success.
        return { exitCode: code ?? (child.signalCode ? 128 + (constants.signals[child.signalCode] ?? 0) : 1) };
      } finally {
        if (pid) untrack(pid);
        if (timer) clearTimeout(timer);
        signal?.removeEventListener('abort', stop);
      }
    },
  };
}

/**
 * The per-call fact for a command that returned while processes it started (`server &`) still run in its group:
 * which ones, and how to stop them. Undefined when the group is gone (always on Windows, which has no groups).
 */
export function strayNote(pid: number): string | undefined {
  if (process.platform === 'win32' || !groupAlive(pid)) return undefined;
  const members = groupMembers(pid);
  if (members.length === 0) return undefined;
  const listed = members.slice(0, LISTED_STRAYS).map((member) => `${member.pid} ${firstLine(member.command).slice(0, 80)}`);
  const more = members.length > LISTED_STRAYS ? `; +${members.length - LISTED_STRAYS} more` : '';
  return `Still running after the command returned (not stopped): ${listed.join('; ')}${more}. Stop: kill -- -${pid}. Use background: true to have such a process's exit reported.`;
}
