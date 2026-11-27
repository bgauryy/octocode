/**
 * Process identity probes: whether a pid still runs, and a start-time token that tells a reused pid from the process
 * that recorded it. Plain TypeScript (strip-only safe), so the build scripts can use it as well as the runtime.
 */
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

const WINDOWS = process.platform === 'win32';

/** What a probe learns about a running pid: whether it is dead-but-unreaped, and an opaque start-time token (null when unknown). */
interface ProcessInfo {
  zombie: boolean;
  start: string | null;
}

/** Linux: /proc state and starttime ticks (no spawn); elsewhere one `ps` for stat and lstart. Null when nothing can be learned. */
function inspectProcess(pid: number): ProcessInfo | null {
  if (WINDOWS) return null;
  if (process.platform === 'linux') {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, 'utf8');
      const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
      return { zombie: fields[0] === 'Z' || fields[0] === 'X', start: fields[19] ?? null };
    } catch {
      return null;
    }
  }
  const result = spawnSync('ps', ['-o', 'stat=,lstart=', '-p', String(pid)], { encoding: 'utf8', timeout: 2000, stdio: ['ignore', 'pipe', 'ignore'], env: { ...process.env, LC_ALL: 'C' } });
  const [, stat = '', start = ''] = result.status === 0 ? (/^\s*(\S+)\s*(.*)$/s.exec(result.stdout.trim()) ?? []) : [];
  return stat === '' ? null : { zombie: stat.startsWith('Z'), start: start.trim() || null };
}

function processExists(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    // EPERM: the process exists but belongs to another user.
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** The start-time token of `pid` (null when unobtainable), to record beside the pid so a reused pid is not mistaken for it. */
export const processStart = (pid: number): string | null => inspectProcess(pid)?.start ?? null;

/** Whether `pid` runs and, when both start times are known, is still the process that started at `start`. */
export function processAlive(pid: number, start: string | null): boolean {
  if (!processExists(pid)) return false;
  // A zombie is dead: `kill(pid, 0)` still succeeds until its parent reaps it.
  const info = inspectProcess(pid);
  if (info === null) return true;
  return !info.zombie && (start === null || info.start === null || info.start === start);
}
