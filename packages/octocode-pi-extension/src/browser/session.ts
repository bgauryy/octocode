import { BROWSER_VISIBLE_ENV, envFlag } from '../shared/env.js';
import { throwIfAborted, unlessAborted } from './abort.js';
import { CdpConnection, type CdpPage } from './cdp.js';
import { checkDevtoolsUrl, openTab, pageTargets, type PageTarget } from './devtools.js';
import { logConsole, noteTabOrigin, pruneClosed, trackDownload, type BrowserSession, type Download } from './session-state.js';
import { applyDownloads } from './emulation.js';
import { closeTarget, launchChrome, persistentProfileDir, profileLocked, profilePort, recordProfilePort, type OwnChrome } from './launch.js';
import { applyPolicy, guardTargets, watchNetwork } from './page-network.js';
import type { PageNetworkPolicy } from './policy.js';
import type { BlockedConnection } from './proxy.js';

interface OpenOptions {
  /** DevTools port to attach to instead of OCTOCODE_CHROME_PORT. */
  port?: number;
  /** Drive the browser's existing page instead of opening a tab (a browser someone else is showing in a window). */
  reuseTab?: boolean;
  /** Checks each request of every page the session drives (links, redirects, script navigations, frames, subresources). */
  policy?: PageNetworkPolicy;
  /** Called for each connection the launched Chrome's filtering proxy refused. */
  onBlocked?: (blocked: BlockedConnection) => void;
  /** The tool call's signal: an abort stops the open at once and releases a Chrome that finishes starting later. */
  signal?: AbortSignal | undefined;
}


export async function openBrowser(env: NodeJS.ProcessEnv = process.env, options: OpenOptions = {}): Promise<BrowserSession> {
  const { signal } = options;
  throwIfAborted(signal);
  // Someone else's Chrome only on an explicit port: an `attach` call, or the user's OCTOCODE_CHROME_PORT opt-in.
  const shared = options.port ?? sharedChromePort(env);
  let chrome: OwnChrome | undefined;
  let headless: boolean | undefined;
  let closeTab: (() => Promise<void>) | undefined;
  let onProfile = false;
  const profile = !options.port && envFlag(env, BROWSER_VISIBLE_ENV) ? persistentProfileDir(env) : undefined;
  // Attaching to the user's Chrome: work in a tab of our own instead of taking over one they have open. A port nobody
  // answers on is no error here: `reuseTab` refuses below, otherwise the session launches its own Chrome.
  let port = shared ?? 0;
  let target = shared === undefined ? undefined : await (options.reuseTab ? firstPage(shared) : newTab(shared)).catch(() => undefined);
  if (options.reuseTab && !target) throw new Error(`No browser is listening on DevTools port ${port}.`);
  if (!target && profile) {
    // Visible mode: the persistent profile's Chrome may still be open from an earlier session (a login hand-off).
    const running = await profilePort(profile);
    if (running) {
      target = await newTab(running);
      port = running;
      onProfile = true;
    } else if (profileLocked(profile)) {
      throw new Error(`The browser profile ${profile} is open in a Chrome without DevTools. Close that Chrome and retry.`);
    }
  }
  if (target && !options.reuseTab) {
    const id = target.id;
    const tabPort = port;
    closeTab = async () => {
      await closeTarget(tabPort, id);
    };
  } else if (!target) {
    // Visible only on the persistent profile (visible mode, a login hand-off); otherwise nobody watches the window.
    headless = !profile;
    const { policy, onBlocked } = options;
    const launching = launchChrome(headless, profile, { ...(onBlocked ? { onBlocked } : {}), ...(policy ? { approved: (host, port) => policy.approves(host, port) } : {}) });
    // A Chrome that finishes starting after an abort has no owner; failing to kill it adds nothing to the abort.
    chrome = await unlessAborted(launching, signal, (late) => late.kill().catch(() => undefined));
    onProfile = profile !== undefined;
    try {
      throwIfAborted(signal);
      // Chrome writes this file only for port 0; chrome-launcher picks a fixed port, so record it (in Chrome's format)
      // for the next run.
      if (profile) await recordProfilePort(profile, chrome.port);
      target = await firstPage(chrome.port);
    } catch (error) {
      await chrome.kill();
      throw error;
    }
  }
  let connection: CdpConnection | undefined;
  try {
    throwIfAborted(signal);
    // Refused before any socket opens, so a hostile target list never gets a connection.
    checkDevtoolsUrl(target.webSocketDebuggerUrl, chrome?.port ?? port);
    connection = await CdpConnection.connect(chrome?.port ?? port);
    const page = await connection.attach(target);
    const kind = chrome ? 'own' : options.reuseTab ? 'attached' : 'shared';
    const session = startSession(connection, page, target, { port: chrome?.port ?? port, kind, chrome, headless, closeTab, profile: onProfile ? profile : undefined, policy: options.policy });
    await preparePage(session, page);
    // Popups, workers and service workers are not part of the page's session: the browser attaches them and holds each until its network check is on.
    if (session.policy) await connection.autoAttach();
    return session;
  } catch (error) {
    // Nothing owns the tab or the Chrome yet: release them instead of leaking a process or a tab.
    connection?.close();
    // The open already failed; a tab that cannot be closed must not replace that error.
    await closeTab?.().catch(() => undefined);
    await chrome?.kill();
    throw error;
  }
}

/** `OCTOCODE_CHROME_PORT` when set: the user's opt-in to share an open Chrome. A value that is not a TCP port is refused, not guessed at. */
export function sharedChromePort(env: NodeJS.ProcessEnv): number | undefined {
  const raw = env['OCTOCODE_CHROME_PORT']?.trim();
  if (!raw) return undefined;
  const port = /^\d+$/.test(raw) ? Number(raw) : Number.NaN;
  if (port >= 1 && port <= 65_535) return port;
  throw new Error(`OCTOCODE_CHROME_PORT=${raw} is not a TCP port (1-65535). Fix or unset it and restart Pi.`);
}

type SessionParts = Pick<BrowserSession, 'port' | 'kind'> & {
  chrome?: OwnChrome | undefined;
  headless?: boolean | undefined;
  closeTab?: (() => Promise<void>) | undefined;
  profile?: string | undefined;
  policy?: PageNetworkPolicy | undefined;
};

function startSession(connection: CdpConnection, page: CdpPage, target: PageTarget, parts: SessionParts): BrowserSession {
  const targetId = target.id;
  const session: BrowserSession = {
    connection,
    page,
    governed: new Map([[targetId, page]]),
    workers: new Map(),
    origins: new Set(),
    tabOrigins: new Map(),
    targetId,
    port: parts.port,
    kind: parts.kind,
    console: [],
    consoleLost: { dropped: 0, cleared: 0 },
    downloads: new Map(),
  };
  noteTabOrigin(session, targetId, target.url);
  if (parts.chrome) session.chrome = parts.chrome;
  if (parts.headless !== undefined) session.headless = parts.headless;
  if (parts.closeTab) session.closeTab = parts.closeTab;
  if (parts.profile) session.profile = parts.profile;
  if (parts.policy) session.policy = parts.policy;
  if (session.policy) {
    guardTargets(session, session.policy);
    // A tab that closes takes its pinned origin and its private-host approval with it, when it happens, not at the next attach.
    connection.onEnded(() => pruneClosed(session));
  }
  watchNetwork(session, page, targetId);
  listen(session, page);
  return session;
}

/**
 * Setup every page the session drives gets: page and runtime events, the download folder (once `download`
 * turned it on), and, with a network policy, a pause before every request (all resource types) so the policy sees each
 * document, frame, subresource and redirect hop. The cache and service workers are off for the page: a cached response
 * or a service worker's answer is never requested, so it would never be paused.
 */
async function preparePage(session: BrowserSession, page: CdpPage): Promise<void> {
  // A tab the session drove before is still attached with all of it on: only the download folder may have been chosen since.
  const again = prepared.has(page);
  if (!again) await Promise.all([page.send('Page.enable'), page.send('Runtime.enable')]);
  if (session.downloadDir) await applyDownloads(page, session.downloadDir);
  if (session.policy && !again) await applyPolicy(page, 'page');
  prepared.add(page);
}

/** The pages `preparePage` finished with: their session stays attached when the session drives another tab, so a return to one needs no second setup. */
const prepared = new WeakSet<CdpPage>();

/** Tracks the page's dialogs, downloads and console output on the session. */
function listen(session: BrowserSession, page: CdpPage): void {
  page.on((method, params) => {
    // A tab the session moved away from stays attached; only the driven one speaks for the session.
    if (page !== session.page && (method === 'Page.javascriptDialogOpening' || method === 'Page.javascriptDialogClosed' || method === 'Runtime.consoleAPICalled' || method === 'Runtime.exceptionThrown')) return;
    if (method === 'Page.javascriptDialogOpening') {
      session.dialog = { type: String(params['type']), message: String(params['message'] ?? '') };
    } else if (method === 'Page.javascriptDialogClosed') {
      delete session.dialog;
    } else if (method === 'Browser.downloadWillBegin' || method === 'Page.downloadWillBegin') {
      trackDownload(session, String(params['guid']), { name: String(params['suggestedFilename'] ?? 'download'), state: 'inProgress', bytes: 0 });
    } else if (method === 'Browser.downloadProgress' || method === 'Page.downloadProgress') {
      const download = session.downloads.get(String(params['guid']));
      const file = typeof params['filePath'] === 'string' && params['filePath'] ? { path: params['filePath'] } : {};
      if (download) trackDownload(session, String(params['guid']), { ...download, ...file, state: params['state'] as Download['state'], bytes: Number(params['receivedBytes'] ?? download.bytes) });
    } else if (method === 'Runtime.consoleAPICalled') {
      const args = Array.isArray(params['args']) ? params['args'] : [];
      const text = args.map((arg) => String((arg as { value?: unknown; description?: string }).value ?? (arg as { description?: string }).description ?? '')).join(' ');
      logConsole(session, `[${String(params['type'])}] ${text}`);
    } else if (method === 'Runtime.exceptionThrown') {
      const details = params['exceptionDetails'] as { text?: string; exception?: { description?: string } } | undefined;
      logConsole(session, `[exception] ${details?.exception?.description ?? details?.text ?? ''}`);
    }
  });
}

/**
 * Moves the session onto another tab: attaches to it (once), applies (once `download` turned them on) the download folder,
 * starts tracking its dialogs, downloads and console, and brings it to the front. The tab the session leaves stays
 * attached and governed, policy and auto-attach on, so what it or the popups and workers it opens later request is still
 * checked: only the pointer to the driven tab moves. A tab's session ends when the tab closes or the browser session ends.
 * The tab the session opened itself is still the one `close` removes.
 */
export async function switchTo(session: BrowserSession, target: PageTarget): Promise<void> {
  pruneClosed(session);
  // A tab the session drove before, or a popup the browser attached before it ran, is already under the policy and watched: it is driven through that session.
  const known = session.governed.get(target.id);
  const page = known ?? (await session.connection.attach(target));
  if (!known) watchNetwork(session, page, target.id);
  const first = !prepared.has(page);
  try {
    await preparePage(session, page);
  } catch (error) {
    if (!known) page.close();
    throw error;
  }
  noteTabOrigin(session, target.id, target.url);
  session.governed.set(target.id, page);
  if (first) listen(session, page);
  session.page = page;
  session.targetId = target.id;
  // The buffer is the bound tab's: what an earlier tab logged is discarded, and the count says so.
  session.consoleLost = { dropped: 0, cleared: session.consoleLost.cleared + session.console.length + session.consoleLost.dropped };
  session.console = [];
  // The log keeps the earlier tab's lines; a line marks where the next tab's begin.
  session.consoleLog?.append(`[tab] console of ${target.url}`);
  delete session.dialog;
  // Cosmetic: the tab is driven whether or not it came to the front.
  await page.send('Page.bringToFront').catch(() => undefined);
}

const newTab = (port: number): Promise<PageTarget> => openTab(port, 'about:blank');

/** The about:blank page of a Chrome this session launched (a new tab when it lists none). */
const firstPage = async (port: number): Promise<PageTarget> => (await pageTargets(port))[0] ?? newTab(port);
