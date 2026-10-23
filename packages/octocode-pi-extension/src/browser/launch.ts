import fs from 'node:fs';
import path from 'node:path';
import { launch, type LaunchedChrome } from 'chrome-launcher';
import { atomicWriteFileSync } from '../shared/atomic.js';
import { HOME_NAMES, outputDir, PRIVATE_FILE_MODE } from '../shared/home.js';
import { processAlive } from '../shared/process.js';
import { sweepDeadOutputs } from '../shared/sweep.js';
import { isRecord } from '../shared/util.js';
import { startFilteringProxy, type FilteringProxy, type FilteringProxyOptions } from './proxy.js';

/** Starting Chrome for a session, and finding the Chrome already running on a persistent profile. */

/** The persistent profile visible mode uses: `<Octocode home>/pi-browser-profile`. */
export const persistentProfileDir = (env: NodeJS.ProcessEnv): string => outputDir(HOME_NAMES.browserProfile, env);

/**
 * A Chrome this session launched; `proxy` is the filtering proxy it runs behind. `kill` stops Chrome, then resolves once
 * what runs with it (the proxy, a fresh profile folder) is released.
 */
export type OwnChrome = Omit<LaunchedChrome, 'kill'> & { kill: () => Promise<void>; proxy?: FilteringProxy };

/** `chrome` with an async `kill`: chrome-launcher's synchronous kill, then `release`, which runs even when the kill throws. */
function owned(chrome: LaunchedChrome, release: () => Promise<void> = async () => undefined): OwnChrome {
  const kill = chrome.kill.bind(chrome);
  return Object.assign(chrome, {
    kill: async (): Promise<void> => {
      try {
        kill();
      } finally {
        await release();
      }
    },
  });
}

/** Chrome's flags to send every request through `proxy`, loopback included (`<-loopback>` removes Chrome's built-in loopback bypass). */
export const proxyFlags = (proxy: FilteringProxy): string[] => [
  `--proxy-server=${proxy.url}`,
  '--proxy-bypass-list=<-loopback>',
];

/**
 * WebRTC's UDP (ICE: STUN and TURN servers a page names, host candidates) does not go through the HTTP proxy or the CDP
 * request guard, so a page could send packets to a private address. Chrome reads this policy from the profile
 * preference `webrtc.ip_handling_policy`; the `--force-webrtc-ip-handling-policy` switch is honored only by content_shell
 * and old headless, not by Chrome (verified: Chrome 155 still sent STUN and TURN datagrams to a loopback server with it).
 * `disable_non_proxied_udp` allows UDP only through a proxy that supports it, so none is sent and no address is gathered.
 */
const WEBRTC_POLICY = 'disable_non_proxied_udp';

/**
 * Sets `webrtc.ip_handling_policy` in the profile's `Default/Preferences`, keeping every other preference, before Chrome
 * starts on `userDataDir` (a running Chrome would overwrite the file). Throws when it cannot: Chrome must not start
 * without the policy.
 */
export function restrictWebRtc(userDataDir: string): void {
  const file = path.join(userDataDir, 'Default', 'Preferences');
  let prefs: Record<string, unknown> = {};
  try {
    const parsed: unknown = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (isRecord(parsed)) prefs = parsed;
  } catch (error) {
    // No file yet (a fresh profile), or one Chrome could not read either and would rebuild.
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error;
  }
  const webrtc = isRecord(prefs['webrtc']) ? prefs['webrtc'] : {};
  fs.mkdirSync(path.dirname(file), { recursive: true });
  atomicWriteFileSync(file, JSON.stringify({ ...prefs, webrtc: { ...webrtc, ip_handling_policy: WEBRTC_POLICY } }), { mode: PRIVATE_FILE_MODE });
}

/**
 * A launched Chrome: `profile` (kept, visible mode) or a fresh profile folder under `<Octocode home>/pi-browser/` (not
 * the temp dir) that is removed when it closes. A fresh Chrome runs behind a filtering proxy that stops with it. One on
 * a kept profile does not: it outlives the session and is reused by the next one, so its coverage is the CDP guard's only.
 */
export async function launchChrome(headless: boolean, profile?: string, proxyOptions?: FilteringProxyOptions): Promise<OwnChrome> {
  // Headless Chrome defaults to an 800x600 window, where many sites collapse into a mobile layout (no search box).
  const flags = [...(headless ? ['--headless=new', '--window-size=1366,900'] : []), '--no-first-run', '--no-default-browser-check'];
  if (profile) {
    restrictWebRtc(profile);
    return owned(await launch({ startingUrl: 'about:blank', userDataDir: profile, chromeFlags: flags }));
  }
  const root = outputDir(HOME_NAMES.browser);
  sweepDeadOutputs(root, 0);
  const proxy = await startFilteringProxy(proxyOptions);
  const userDataDir = fs.mkdtempSync(path.join(root, `${process.pid}-`));
  let chrome: LaunchedChrome;
  try {
    restrictWebRtc(userDataDir);
    chrome = await launch({ startingUrl: 'about:blank', userDataDir, chromeFlags: [...flags, ...proxyFlags(proxy)] });
  } catch (error) {
    await proxy.close();
    fs.rmSync(userDataDir, { recursive: true, force: true });
    throw error;
  }
  const own = owned(chrome, async () => {
    await proxy.close();
    fs.rmSync(userDataDir, { recursive: true, force: true, maxRetries: 5 });
  });
  own.proxy = proxy;
  return own;
}

/**
 * The DevTools port of a Chrome already running on `profile`, from the DevToolsActivePort file there: the port, then
 * the browser's DevTools path (`/devtools/browser/<id>`, new for every Chrome run). A file whose port now answers with
 * another path is stale (that Chrome quit and something else took the port): it is removed, and nothing is reused.
 */
export async function profilePort(profile: string): Promise<number | undefined> {
  const file = path.join(profile, 'DevToolsActivePort');
  let lines: string[];
  try {
    lines = fs.readFileSync(file, 'utf8').split('\n');
  } catch {
    return undefined;
  }
  const port = Number(lines[0]);
  if (Number.isInteger(port) && port > 0 && lines[1] && (await browserPath(port, 1_000).catch(() => undefined)) === lines[1].trim()) return port;
  fs.rmSync(file, { force: true });
  return undefined;
}

/** Records `port` in the profile's DevToolsActivePort the way Chrome does (it writes the file only for port 0). */
export async function recordProfilePort(profile: string, port: number): Promise<void> {
  fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${port}\n${await browserPath(port, 5_000)}\n`);
}

/** A request to the DevTools HTTP endpoint (`/json/...`) of the browser on `port`, always on this machine. */
export const devtoolsFetch = (port: number, endpoint: string, timeoutMs: number, init: RequestInit = {}): Promise<Response> =>
  fetch(`http://127.0.0.1:${port}${endpoint}`, { ...init, signal: AbortSignal.timeout(timeoutMs) });

/** Closes the tab `id` of the browser on `port`; whether the browser said it did (the tab may be gone already). */
export const closeTarget = (port: number, id: string): Promise<boolean> => devtoolsFetch(port, `/json/close/${id}`, 2_000).then((response) => response.ok, () => false);

/** The browser-level DevTools WebSocket URL of the browser on `port`, from `/json/version`: the one socket a session drives every page through. */
export async function browserWebSocketUrl(port: number, timeoutMs: number): Promise<string> {
  const response = await devtoolsFetch(port, '/json/version', timeoutMs);
  const { webSocketDebuggerUrl } = (await response.json()) as { webSocketDebuggerUrl?: unknown };
  if (typeof webSocketDebuggerUrl !== 'string') throw new Error(`Chrome on port ${port} reports no browser DevTools URL`);
  return webSocketDebuggerUrl;
}

/** The DevTools path of the browser on `port` (`/devtools/browser/<id>`). */
const browserPath = async (port: number, timeoutMs: number): Promise<string> => new URL(await browserWebSocketUrl(port, timeoutMs)).pathname;

/** Chrome's SingletonLock (a `<host>-<pid>` symlink) names a live process: another Chrome owns the profile. */
export function profileLocked(profile: string): boolean {
  try {
    const pid = Number(fs.readlinkSync(path.join(profile, 'SingletonLock')).split('-').pop());
    return Number.isInteger(pid) && pid > 0 && processAlive(pid);
  } catch {
    return false;
  }
}
