import { errorMessage, withTimeout } from '../shared/util.js';
import { closeTarget } from './launch.js';
import type { AttachedTarget, CdpPage } from './cdp.js';
import { httpOrigin, logConsole, noteOrigin, noteTabOrigin, pruneClosed, type BrowserSession } from './session-state.js';
import type { PageNetworkPolicy, RequestSource } from './policy.js';

/**
 * Enforces the page-request policy on the targets a session drives: each paused request, a response from an address the policy
 * did not allow, a WebSocket it cannot stop, and every target the browser attaches by itself. The policy decides (`network.ts`);
 * this is where its verdicts are applied to the browser.
 */

/** Targets that load documents: their setup also turns the cache and service workers off (see `preparePage` in `session.ts`). */
const PAGE_TYPES = new Set(['page', 'iframe']);

/**
 * What a target needs for the policy to see every request it makes, sent in order and all at once: Chrome runs a session's
 * commands in the order it got them, so a command sent before `Runtime.runIfWaitingForDebugger` is applied before the target
 * runs, and a held service worker answers none of them until it is released. A dedicated worker has no `Fetch` domain: the
 * session of the page that owns it pauses its requests. The target's own related targets (frames in another process,
 * workers) are attached to as well, each held until its own setup is done.
 */
export function applyPolicy(page: CdpPage, type: string): Promise<unknown> {
  const sent: Array<Promise<unknown>> = [page.send('Network.enable')];
  if (PAGE_TYPES.has(type)) sent.push(page.send('Network.setCacheDisabled', { cacheDisabled: true }), page.send('Network.setBypassServiceWorker', { bypass: true }));
  if (type !== 'worker') sent.push(page.send('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] }));
  sent.push(page.autoAttach());
  return Promise.all(sent);
}

/** Targets the policy governs; any other the browser attaches (an extension's page, a worklet) is let run and left alone. */
const GOVERNED_TYPES = new Set(['page', 'iframe', 'worker', 'shared_worker', 'service_worker']);

/** Workers that live per origin, not per page: the browser attaches them on its own session, so the page that started one is known by its origin. */
const ORIGIN_TYPES = new Set(['shared_worker', 'service_worker']);

/** Whether the session `sessionId` belongs to a tab, worker or frame the policy governs. */
function drivenSession(session: BrowserSession, sessionId: string | undefined): boolean {
  if (sessionId === undefined) return false;
  for (const pages of [session.governed, session.workers]) for (const page of pages.values()) if (page.sessionId === sessionId && page.open) return true;
  return false;
}

/**
 * Whether a target the browser attached belongs to the tabs the session drives: a popup whose opener is governed, a
 * worker or frame whose parent session is governed, a service or shared worker whose origin a governed tab loaded.
 */
function relatedToDriven(session: BrowserSession, { info, parentSessionId }: AttachedTarget): boolean {
  if (info.type === 'page') return info.openerId !== undefined && session.governed.has(info.openerId);
  if (drivenSession(session, parentSessionId)) return true;
  if (!ORIGIN_TYPES.has(info.type)) return false;
  try {
    return session.origins.has(new URL(info.url ?? '').origin);
  } catch {
    return false;
  }
}

/**
 * Whether the browser may hold targets that are not the session's: a Chrome it did not launch (`attach`,
 * `OCTOCODE_CHROME_PORT`) is the user's own, with tabs and workers of their own that the policy must never touch. The Chrome
 * on the persistent profile is the agent's browser, started by it or by an earlier session.
 */
const sharesBrowser = (session: BrowserSession): boolean => session.kind !== 'own' && session.profile === undefined;

/**
 * Sets up each target the browser attaches by itself before it runs a line: its network setup is sent while it waits for
 * the debugger, and only then is it let go. That is what puts a popup's first request, a worker's fetches and a service
 * worker's own fetches (which no page session ever pauses) under the policy. The session's own Chrome is covered whole.
 * In the user's Chrome only the targets related to a tab the session drives are (see `relatedToDriven`); every other one is
 * let run at once and detached untouched. A tab the browser opened that the session did not (the user's own tabs, one it is
 * about to attach to itself) is let run and detached. A target whose setup fails is closed.
 */
export function guardTargets(session: BrowserSession, policy: PageNetworkPolicy): void {
  session.connection.onAttached((attached) => {
    void guardAttached(session, policy, attached);
  });
}

async function guardAttached(session: BrowserSession, policy: PageNetworkPolicy, attached: AttachedTarget): Promise<void> {
  const { page, info, waiting } = attached;
  // A failed release (the target is gone) leaves nothing to run, so it is not an error.
  const resume = (): Promise<void> => (waiting ? page.send('Runtime.runIfWaitingForDebugger').then(() => undefined, () => undefined) : Promise.resolve());
  pruneClosed(session);
  const governable = GOVERNED_TYPES.has(info.type) && !info.url?.startsWith('chrome-extension:');
  const mine = governable && (info.type === 'page' ? !session.governed.has(info.targetId) && relatedToDriven(session, attached) : !sharesBrowser(session) || relatedToDriven(session, attached));
  if (!mine) {
    await resume();
    page.close();
    return;
  }
  const tab = info.type === 'page' ? info.targetId : undefined;
  if (tab && info.openerId) policy.opened(info.openerId, tab);
  // Registered before the setup: what this target opens next is announced as soon as it runs, and finds its parent governed.
  (tab ? session.governed : session.workers).set(info.targetId, page);
  watchNetwork(session, page, tab, tab ? undefined : workerSource(session, attached));
  // The release goes out right behind the setup; a failed setup is found when its replies are in, and the target is closed.
  const setup = applyPolicy(page, info.type);
  const released = resume();
  try {
    await setup;
  } catch (error) {
    const reason = `could not set up the network check (${errorMessage(error)})`;
    (tab ? session.governed : session.workers).delete(info.targetId);
    session.blocked = `${info.url ?? info.type}: ${reason}`;
    logConsole(session, `[blocked] ${session.blocked}`);
    policy.report({ kind: 'blocked', url: info.url ?? '', resourceType: info.type, reason });
    // Best effort: a target that cannot be closed is already unusable (it was removed from the governed set above).
    await session.connection.send('Target.closeTarget', { targetId: info.targetId }).catch(() => undefined);
    return;
  }
  await released;
}

/** Who made the requests of each page the session watches (`watchNetwork`). */
const sources = new WeakMap<CdpPage, RequestSource>();

/** The page of the session `sessionId`, among the tabs, workers and frames the session governs. */
function pageOfSession(session: BrowserSession, sessionId: string | undefined): CdpPage | undefined {
  if (sessionId === undefined) return undefined;
  for (const pages of [session.governed, session.workers]) for (const page of pages.values()) if (page.sessionId === sessionId) return page;
  return undefined;
}

/**
 * Who a worker or frame in another process requests for: the one whose page owns it (a tab's frames and dedicated workers
 * are the tab's own), else, for a service or shared worker, the origin it lives at.
 */
function workerSource(session: BrowserSession, { info, parentSessionId }: AttachedTarget): RequestSource {
  const parent = pageOfSession(session, parentSessionId);
  const inherited = parent && sources.get(parent);
  if (inherited) return inherited;
  const origin = ORIGIN_TYPES.has(info.type) ? httpOrigin(info.url) : undefined;
  return origin === undefined ? {} : { origin };
}

/** How long a paused request waits for the policy's verdict before it fails: a request nobody answers must not hang the page. */
const REQUEST_VERDICT_MS = 5_000;

/** Lets a paused request through when the policy allows its URL; fails it otherwise (also when the verdict takes too long) and records why. */
async function screenRequest(session: BrowserSession, page: CdpPage, policy: PageNetworkPolicy, source: RequestSource, params: Record<string, unknown>): Promise<void> {
  const requestId = String(params['requestId']);
  const url = String((params['request'] as { url?: unknown } | undefined)?.url ?? '');
  const resourceType = String(params['resourceType'] ?? 'Other');
  try {
    await withTimeout(policy.check(url, resourceType, source), REQUEST_VERDICT_MS, 'The request check');
  } catch (error) {
    const reason = errorMessage(error);
    session.blocked = `${url}: ${reason}`;
    logConsole(session, `[blocked] ${session.blocked}`);
    policy.report({ kind: 'blocked', url, resourceType, reason });
    // The request may be gone already (the tab navigated or closed), and the refusal is recorded above.
    await page.send('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }).catch(() => undefined);
    return;
  }
  // The request may be gone (the tab navigated elsewhere or closed) by the time the policy answers.
  await page.send('Fetch.continueRequest', { requestId }).catch(() => undefined);
}

/**
 * A response that came from a private address for a name the policy had judged public (DNS rebinding): the request
 * already reached it, so its tab (`tab`, the target id) is closed rather than driven further. Not checked behind the filtering proxy: Chrome
 * sees the proxy's address there, and the proxy vets the address itself.
 */
async function closeRebound(session: BrowserSession, policy: PageNetworkPolicy, url: string, address: string, tab: string | undefined): Promise<void> {
  policy.report({ kind: 'rebinding', url, address });
  if (!tab) {
    // A worker has no tab to close.
    logConsole(session, `[blocked] ${url} answered from the private address ${address}`);
    return;
  }
  // Best effort (the tab may be gone already): the log says whether it closed, not that it was asked to.
  const closed = await closeTarget(session.port, tab);
  logConsole(session, `[blocked] ${url} answered from the private address ${address}; ${closed ? 'the tab was closed' : 'the tab could not be closed'}`);
}

/** A WebSocket the page opened to a host it may not reach. `Fetch` never pauses a handshake, so it is reported, not stopped (a launched Chrome's proxy stops it). */
async function reportWebSocket(session: BrowserSession, policy: PageNetworkPolicy, source: RequestSource, url: string): Promise<void> {
  try {
    await policy.check(url, 'WebSocket', source);
  } catch (error) {
    const reason = errorMessage(error);
    session.warning = `The page opened a WebSocket it may not (${url}): ${reason} It cannot be stopped on this browser; it may already have exchanged data.`;
    logConsole(session, `[warning] ${session.warning}`);
    policy.report({ kind: 'websocket', url, reason });
  }
}

/**
 * Applies the policy to what a target's session reports: each paused request, a response from an address the policy did not
 * allow, a WebSocket it cannot stop. `tab` is the target id when the target is a tab, which a rebinding closes; `source` says
 * who the requests are for (the tab itself by default).
 */
export function watchNetwork(session: BrowserSession, page: CdpPage, tab: string | undefined, source: RequestSource = tab === undefined ? {} : { tab }): void {
  const { policy } = session;
  if (!policy) return;
  sources.set(page, source);
  page.on((method, params) => {
    if (method === 'Fetch.requestPaused') {
      if (params['resourceType'] === 'Document') noteOrigin(session, String((params['request'] as { url?: unknown } | undefined)?.url ?? ''));
      void screenRequest(session, page, policy, source, params);
    } else if (method === 'Page.frameNavigated' && tab !== undefined) {
      // The tab's main frame committed: its approval for a private host ends when it left that host, and its origin is pinned.
      const frame = params['frame'] as { url?: unknown; parentId?: unknown } | undefined;
      if (frame && frame.parentId === undefined && typeof frame.url === 'string') {
        policy.navigated(tab, frame.url);
        noteTabOrigin(session, tab, frame.url);
      }
    } else if (method === 'Network.responseReceived' && !session.chrome?.proxy) {
      const response = params['response'] as { url?: unknown; remoteIPAddress?: unknown; remotePort?: unknown } | undefined;
      const { url, remoteIPAddress: address } = response ?? {};
      if (typeof url === 'string' && typeof address === 'string' && policy.rebinds(url, address, typeof response?.remotePort === 'number' ? response.remotePort : undefined)) {
        void closeRebound(session, policy, url, address, tab);
      }
    } else if (method === 'Network.webSocketCreated' && !session.chrome?.proxy) {
      void reportWebSocket(session, policy, source, String(params['url'] ?? ''));
    }
  });
}
