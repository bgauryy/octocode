import { onSecond } from '../shared/clock.js';
import { parentGone, sameProcess } from '../shared/process.js';

/** Every this many parent checks (one a second, on the shared clock) the start time is compared too: a reused pid is noticed within that many seconds, and the event loop is not stalled by a `ps` every second. */
const PARENT_START_EVERY = 15;

/**
 * Whether the process named by `OCTOCODE_PARENT_PID` (`<pid> <start time>`, the start time when known) is gone. A
 * direct parent is judged by the pid and reparenting; through a launcher, a recorded start time tells a later process
 * that reused the pid from the parent.
 */
export function parentProcessGone(value: string, compareStart = true): boolean {
  const [pid = '', ...started] = value.trim().split(' ');
  const parentPid = Number(pid);
  if (!Number.isInteger(parentPid) || parentPid <= 0) return false;
  if (parentGone(parentPid)) return true;
  // The start time costs a synchronous `ps`, so a caller that polls compares it only now and then (`compareStart`).
  return compareStart && process.ppid !== parentPid && started.length > 0 && !sameProcess(parentPid, started.join(' '));
}

/** Checks the parent `value` names once a second, on the shared clock; stops and calls `gone` once it is gone. Returns what stops the check. */
export function watchParentProcess(value: string, gone: () => void): () => void {
  let checks = 0;
  const stop = onSecond(() => {
    if (!parentProcessGone(value, checks++ % PARENT_START_EVERY === 0)) return;
    stop();
    gone();
  });
  return stop;
}
