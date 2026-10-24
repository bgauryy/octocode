// Isolated browser sessions this package launches: one session file and one profile folder per
// port under <workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/, and ownership
// checks that never trust a listener or a pid file alone.
import { readFileSync } from 'fs';
import { join, resolve } from 'path';
import { cdpHttp } from './engine-io.mjs';
import { browserPid, getProcessCommand } from './browser-process.mjs';

export const STATE_SUBPATH = [
  '.octocode',
  'octocode-chrome-devtools',
  'tmp',
  'browser-state',
];
export const sessionFileIn = (stateDir: string, port: string) =>
  join(stateDir, `cdp-session-${port}.json`);
export const profileDirIn = (stateDir: string, port: string) =>
  join(stateDir, `cdp-chrome-profile-${port}`);

/** The /json/version fields this package reads. */
export type ChromeVersion = { Browser: string; webSocketDebuggerUrl: string };

/** The session file open-browser writes for an isolated browser it launched. */
export type TrackedSession = {
  pid: number;
  // open-browser writes the port as a string; readers compare String(port) all the same.
  port: string | number;
  profileDir?: string;
  headless?: boolean;
  isolated?: boolean;
  startedAt?: number;
  chromePath?: string;
};

/** Chrome's /json/version on port, or null when nothing answers within 2s. */
export async function checkRunning(
  port: string
): Promise<ChromeVersion | null> {
  try {
    return await cdpHttp(port, '/json/version', { timeoutMs: 2000 });
  } catch {
    return null;
  }
}

export function readSession(sessionFile: string): TrackedSession | null {
  try {
    return JSON.parse(readFileSync(sessionFile, 'utf8'));
  } catch {
    return null;
  }
}

function escapeRegExp(text: string) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// The isolated profile this package created for port, read from a Chrome
// command line: <workspace>/.octocode/octocode-chrome-devtools/tmp/browser-state/cdp-chrome-profile-<port>.
function packageProfileFromCommand(command: string, port: string) {
  const sep = '[\\\\/]';
  const suffix = [...STATE_SUBPATH, `cdp-chrome-profile-${port}`]
    .map(escapeRegExp)
    .join(sep);
  const match = command.match(
    new RegExp(`--user-data-dir=(.+?)${sep}${suffix}(?=["\\s]|$)`)
  );
  // A lazy match must not span into another flag's value.
  if (!match || /\s--/.test(match[1])) return null;
  const workspace = match[1].replace(/^"/, '');
  const stateDir = join(workspace, ...STATE_SUBPATH);
  return { workspace, stateDir, profileDir: profileDirIn(stateDir, port) };
}

/** Why no session owns the listener, or the verified session that does. */
type ListenerSession =
  | {
      reason: string;
      listenerPid?: number;
      workspace?: string;
      sessionFile?: undefined;
      session?: undefined;
    }
  | {
      reason?: undefined;
      listenerPid: number;
      workspace: string;
      sessionFile: string;
      session: TrackedSession;
    };

const sameSession = (
  session: TrackedSession | null,
  port: string,
  profileDir: string
): session is TrackedSession =>
  session !== null &&
  String(session.port) === port &&
  resolve(session.profileDir ?? '') === resolve(profileDir);

// Finds the workspace that launched the Chrome listening on port, when it is an
// isolated profile this package created. Returns its session for the normal
// verified cleanup; never trusts the listener alone.
export async function discoverListenerSession(
  info: ChromeVersion | null,
  port: string
): Promise<ListenerSession> {
  const running = info ?? (await checkRunning(port));
  if (!running) return { reason: 'NO_LISTENER' };
  const pid = await browserPid(running.webSocketDebuggerUrl);
  if (!pid) return { reason: 'LISTENER_PID_UNKNOWN' };
  const owner = packageProfileFromCommand(getProcessCommand(pid), port);
  if (!owner) return { listenerPid: pid, reason: 'NOT_PACKAGE_PROFILE' };
  const sessionFile = sessionFileIn(owner.stateDir, port);
  const session = readSession(sessionFile);
  if (!sameSession(session, port, owner.profileDir))
    return {
      listenerPid: pid,
      workspace: owner.workspace,
      reason: 'NO_SESSION_FILE',
    };
  return { listenerPid: pid, workspace: owner.workspace, sessionFile, session };
}

/** Cleanup only kills a Chrome launched with this exact CDP port and isolated profile path; never a stale pid alone. */
export function processMatchesTrackedSession(session: TrackedSession) {
  const command = getProcessCommand(session.pid);
  if (!command) return false;
  const isChrome =
    /Chrome|Chromium|Brave Browser|chrome|chromium|chrome\.exe/i.test(command);
  const hasPort = command.includes(`--remote-debugging-port=${session.port}`);
  const hasProfile = session.profileDir && command.includes(session.profileDir);
  return isChrome && hasPort && hasProfile;
}
