// Where this workspace's browser sessions are recorded, and which one a command without an explicit
// port should use. Reads files and probes localhost only, so the sandboxed runner can import it.
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'fs';
import { join } from 'path';
import { cdpHttp } from './engine-io.mjs';

export const STATE_SUBPATH = [
  '.octocode',
  'octocode-chrome-devtools',
  'tmp',
  'browser-state',
];
export const DEFAULT_PORT = '9222';
export const stateDirFor = (workspace: string) =>
  join(workspace, ...STATE_SUBPATH);
export const sessionFileIn = (stateDir: string, port: string) =>
  join(stateDir, `cdp-session-${port}.json`);
export const profileDirIn = (stateDir: string, port: string) =>
  join(stateDir, `cdp-chrome-profile-${port}`);
export const activeFileIn = (stateDir: string) =>
  join(stateDir, 'active-session.json');

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
  wsUrl?: string;
};

/** The browser the last `open` made ready, whether launched or reused. */
export type ActiveSession = {
  port: string;
  pid?: number;
  wsUrl: string;
  openedAt: number;
  reused: boolean;
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

function readJson(file: string): unknown {
  try {
    return JSON.parse(readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}

const isSession = (value: unknown): value is TrackedSession =>
  typeof value === 'object' &&
  value !== null &&
  'pid' in value &&
  'port' in value;

/** The session file open-browser wrote; a damaged or foreign file reads as no session. */
export function readSession(sessionFile: string): TrackedSession | null {
  const value = readJson(sessionFile);
  return isSession(value) ? value : null;
}

const isActive = (value: unknown): value is ActiveSession =>
  typeof value === 'object' &&
  value !== null &&
  'port' in value &&
  'openedAt' in value;

/** `isolated` means not the real profile; a visible launch keeps its profile per port too. */
export const isolatedProfileNote = (headless: boolean, port: string) =>
  `${headless ? 'Headless' : 'Visible'} isolated profile, persistent for port ${port} until cleanup; not your real browser profile.`;

/** Records the browser `open` made ready, so later commands without a port find it. */
export function recordActiveSession(
  stateDir: string,
  active: Omit<ActiveSession, 'openedAt'>
) {
  mkdirSync(stateDir, { recursive: true, mode: 0o700 });
  writeFileSync(
    activeFileIn(stateDir),
    JSON.stringify({ ...active, openedAt: Date.now() }),
    { mode: 0o600 }
  );
}

export type RecordedSession = {
  port: string;
  pid?: number;
  wsUrl?: string;
  openedAt: number;
};

/** Every session this workspace recorded (the active file and each per-port session file), newest first. */
export function recordedSessions(stateDir: string): RecordedSession[] {
  const byPort = new Map<string, RecordedSession>();
  let names: string[] = [];
  try {
    names = readdirSync(stateDir);
  } catch {
    return [];
  }
  for (const name of names) {
    const port = /^cdp-session-(\d+)\.json$/.exec(name)?.[1];
    const session = port && readSession(join(stateDir, name));
    if (port && session)
      byPort.set(port, {
        port,
        pid: session.pid,
        wsUrl: session.wsUrl,
        openedAt: session.startedAt ?? 0,
      });
  }
  const active = readJson(activeFileIn(stateDir));
  if (isActive(active))
    byPort.set(active.port, {
      port: active.port,
      pid: active.pid,
      wsUrl: active.wsUrl,
      openedAt: active.openedAt,
    });
  return [...byPort.values()].toSorted((a, b) => b.openedAt - a.openedAt);
}

/** Recorded sessions whose browser answers now, newest first. */
export async function liveSessions(stateDir: string, probe = checkRunning) {
  const recorded = recordedSessions(stateDir);
  const answers = await Promise.all(recorded.map(s => probe(s.port)));
  return recorded.filter((_, index) => answers[index]);
}

/** Why a probe of Chrome's DevTools endpoint failed, in words an agent can act on. */
export function probeFailure(error: unknown): string {
  if (!(error instanceof Error)) return String(error);
  if (error.name === 'TimeoutError' || error.name === 'AbortError')
    return 'timed out';
  const status = /CDP HTTP (\d+)/.exec(error.message)?.[1];
  if (status) return `HTTP status ${status}`;
  const cause = error.cause instanceof Error ? error.cause : undefined;
  const code = cause && 'code' in cause ? String(cause.code) : undefined;
  if (code === 'ECONNREFUSED') return 'connection refused';
  return code ?? cause?.message ?? error.message;
}

/** The message for a command whose port has no Chrome, naming live sessions this workspace recorded. */
export function noChromeMessage(
  port: string,
  cause: string,
  live: readonly { port: string }[]
) {
  const others = live.filter(s => s.port !== port).map(s => s.port);
  const hint = others.length
    ? `A browser opened by this workspace runs on ${others.join(', ')}: pass connection.port ${others[0]}, or call open.`
    : 'Call open to launch one, or pass connection.port of a running Chrome.';
  return `No Chrome on port ${port} (${cause}). ${hint}`;
}

export type ResolvedPort = {
  port: string;
  source: 'explicit' | 'recorded' | 'default';
};

/** The explicit port, else the newest live recorded session's, else 9222. */
export async function resolvePort(
  explicit: string | undefined,
  stateDir: string,
  probe = checkRunning
): Promise<ResolvedPort> {
  if (explicit !== undefined) return { port: explicit, source: 'explicit' };
  const [latest] = await liveSessions(stateDir, probe);
  return latest
    ? { port: latest.port, source: 'recorded' }
    : { port: DEFAULT_PORT, source: 'default' };
}
