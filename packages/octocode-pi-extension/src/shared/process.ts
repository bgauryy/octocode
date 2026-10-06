import { spawn } from 'node:child_process';

/** How long a stopped process tree gets after SIGTERM before SIGKILL. */
export const KILL_GRACE_MS = 500;

/** Whether a process with this pid exists (EPERM: it exists but belongs to someone else). Non-positive or non-integer pids are not alive. */
export function processAlive(pid: unknown): boolean {
  if (typeof pid !== 'number' || !Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/**
 * Sends `signal` to the process tree rooted at `pid`: its process group on POSIX (spawn it with `detached: true` so it
 * leads one; otherwise only the process itself is signalled) and `taskkill /T /F` on Windows. Never throws.
 */
export function signalTree(pid: number | undefined, signal: NodeJS.Signals): void {
  if (!pid) return;
  if (process.platform === 'win32') {
    try {
      spawn('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true }).on('error', () => undefined);
    } catch {
      // taskkill is missing; nothing more to do.
    }
    return;
  }
  try {
    process.kill(-pid, signal);
  } catch {
    try {
      process.kill(pid, signal);
    } catch {
      // Already gone.
    }
  }
}

/** SIGTERM to the tree now and SIGKILL after `graceMs` (an unref'd timer, so it never holds the process open). */
export function killTree(pid: number | undefined, graceMs = KILL_GRACE_MS): void {
  if (!pid) return;
  signalTree(pid, 'SIGTERM');
  setTimeout(() => signalTree(pid, 'SIGKILL'), graceMs).unref();
}
