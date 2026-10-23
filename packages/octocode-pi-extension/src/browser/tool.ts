import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { checkBrowserUrl, type Confirmer } from './access.js';
import { dialogNotice } from './actions.js';
import { browserCallHeader, browserResult } from './render.js';
import { PagePolicy } from './network.js';
import { PAGE_HANDLERS, pageResult, type ActionResult } from './page-handlers.js';
import { PARAMETERS, type BrowserAction, type BrowserParams, type SessionAction } from './params.js';
import type { NetworkEvent } from './policy.js';
import type { BlockedConnection } from './proxy.js';
import { openBrowser } from './session.js';
import type { BrowserSession } from './session-state.js';
import { snapshot } from './snapshot.js';
import type { Approval } from './tabs.js';
import type { Permission } from '../permission/ask.js';
import { timedTool } from '../shared/tool-timing.js';
import { visibleError } from '../shared/visible-error.js';
import type { HostLookup } from '../web/guard.js';

/** Actions that talk to the page, so an open JavaScript dialog must be answered first. */
const PAGE_ACTIONS = new Set(['click', 'hover', 'drag', 'type', 'fill', 'press', 'wait', 'evaluate', 'upload', 'download']);

/** The refusals and reports kept apart in the audit (`BrowserTool.#audited`), most distinct ones, and how soon a repeat count is written. */
export const AUDIT_KEYS_MAX = 100;
const AUDIT_FLUSH_MS = 10_000;
const OVERFLOW_KEY = 'more distinct refusals';

interface Audited {
  repeats: number;
  /** The repeat count the audit already holds. */
  written: number;
  write: (repeats: number) => void;
}

/** Before any tool call supplies a context: nothing to ask, so what needs confirmation is refused. */
const NO_UI = { hasUI: false } as Confirmer;

export class BrowserTool {
  #session: BrowserSession | undefined;
  /** The latest tool call's context: where the navigation guard asks for confirmation. */
  #confirmer: Confirmer = NO_UI;
  /** The latest tool call's id: what the asks the call raises are tied to. */
  #callId: string | undefined;
  /** What the session's pages may request: every request, not only URLs the agent passes. Private hosts only when the agent opened them. */
  readonly #policy: PagePolicy;
  /**
   * Refusals and reports already in the audit, by what tells one from another (kind, host and port, resource type,
   * reason). A page that repeats one writes it once; the repeats are counted and audited as a summary shortly after
   * (`AUDIT_FLUSH_MS`) and again when the browser session ends, so a crash loses at most the last interval's counts.
   * At `AUDIT_KEYS_MAX` distinct entries the others are counted under one entry that says how many more distinct
   * refusals there were (`OVERFLOW_KEY`), so a page cannot flood the audit and nothing is dropped silently.
   */
  readonly #audited = new Map<string, Audited>();
  #flushTimer: NodeJS.Timeout | undefined;
  /** Resolves host names for the check on the pages the agent opens, and for what pages request; the `web` tool's resolver by default. */
  readonly #lookup: HostLookup | undefined;
  /** What the last session lost before it ended (a tab closed for DNS rebinding), told with the next session's first result. */
  #notice: string | undefined;

  readonly #flushMs: number;

  /** `permission` asks about the pages the agent opens, private hosts and uploads; its session grants remember the hosts the user allowed. `flushMs`: how soon a repeat count is audited (a seam for tests). */
  constructor(readonly permission: Permission, options: { lookup?: HostLookup; flushMs?: number } = {}) {
    this.#lookup = options.lookup;
    this.#flushMs = options.flushMs ?? AUDIT_FLUSH_MS;
    this.#policy = new PagePolicy({ report: (event) => this.#audit(event), ...(options.lookup ? { lookup: options.lookup } : {}) });
  }

  /**
   * Whether the agent may open `url` (navigate, a new tab, switching to a tab): the page's own ask, one prompt for a
   * private host. The host is classified once, there; a private target the permission allowed is the one the page may
   * load from, and the launched Chrome's proxy lets it through. The approval it returns is recorded for the tab that loads
   * the page (`approve(tab)`): it lasts for the whole browser session when the user granted `net.private` for the
   * session, otherwise (Allow once, or a mode that allows it) while that tab stays on the host. A public one is never
   * approved: if its name answers privately later, the page request, the proxy and the rebinding check refuse it.
   */
  async open(url: string): Promise<Approval> {
    const { privateGrant } = await checkBrowserUrl(url, this.#confirmer, this.permission, { toolCallId: this.#callId ?? '', lookup: this.#lookup });
    if (!privateGrant) return () => undefined;
    const target = new URL(url);
    return (tab) => this.#policy.approve(target, privateGrant === 'session' ? 'session' : { tab });
  }

  /** One audit entry per distinct refusal or report: a page with many identical blocked requests is one entry and a count, not many entries. */
  #audit(event: NetworkEvent): void {
    if (event.kind === 'rebinding') {
      // Reported before the close is attempted; the console log says whether it succeeded (`page-network.ts`, closeRebound).
      this.#notice = `Closing the tab: ${event.url} answered from the private address ${event.address} (DNS rebinding).`;
      this.permission.refused([`${event.url} -> ${event.address}`], 'closing the tab: a public name answered from a private address (DNS rebinding)', 'browser');
    } else if (event.kind === 'blocked') {
      this.#auditOnce(`blocked ${hostOf(event.url)} ${event.resourceType} ${event.reason}`, (repeats) =>
        this.permission.refused([event.url], `${repeatNote(repeats)}blocked a page request (${event.resourceType}): ${event.reason}`, 'browser'),
      );
    } else {
      this.#auditOnce(`websocket ${hostOf(event.url)} ${event.reason}`, (repeats) =>
        this.permission.noted([event.url], `${repeatNote(repeats)}a page WebSocket was not blocked, reported only: ${event.reason}`, { allowed: true, via: 'reported', action: 'invariant', tool: 'browser' }),
      );
    }
  }

  /** A connection the launched Chrome's filtering proxy refused (a page chose a private or unvetted host). */
  #auditProxy(blocked: BlockedConnection): void {
    this.#auditOnce(`proxy ${blocked.host} ${blocked.reason}`, (repeats) =>
      this.permission.refused([blocked.host], `${repeatNote(repeats)}the filtering proxy refused a page connection to ${blocked.reason === 'private' ? 'a local or private address' : 'a host it could not vet'}`, 'browser'),
    );
  }

  /** Writes `write(0)` the first time `key` is seen; later ones are only counted, for `#flushRepeats`. Past `AUDIT_KEYS_MAX` distinct keys a new one is counted under `OVERFLOW_KEY`. */
  #auditOnce(key: string, write: (repeats: number) => void): void {
    const overflow = !this.#audited.has(key) && this.#audited.size >= AUDIT_KEYS_MAX;
    const name = overflow ? OVERFLOW_KEY : key;
    const known = this.#audited.get(name);
    if (known) {
      known.repeats += 1;
      this.#scheduleFlush();
      return;
    }
    const writer = overflow
      ? (repeats: number) => this.permission.refused([`more than ${AUDIT_KEYS_MAX} distinct blocked targets`], `${repeats + 1} more distinct refusals or reports after the first ${AUDIT_KEYS_MAX}, counted here and not listed one by one`, 'browser')
      : write;
    this.#audited.set(name, { repeats: 0, written: 0, write: writer });
    writer(0);
  }

  /** Audits the counts that grew soon, so a crash or a kill loses little: repeats are cumulative, a later summary replaces an earlier one. */
  #scheduleFlush(): void {
    if (this.#flushTimer) return;
    this.#flushTimer = setTimeout(() => {
      this.#flushTimer = undefined;
      this.#flushRepeats();
    }, this.#flushMs);
    this.#flushTimer.unref();
  }

  /** Audits the count of each entry that repeated since it was last written. */
  #flushRepeats(): void {
    for (const entry of this.#audited.values()) {
      if (entry.repeats <= entry.written) continue;
      entry.written = entry.repeats;
      entry.write(entry.repeats);
    }
  }

  /** Audits what is left and starts afresh: the browser session the entries belong to ended. */
  #endAudit(): void {
    clearTimeout(this.#flushTimer);
    this.#flushTimer = undefined;
    this.#flushRepeats();
    this.#audited.clear();
  }

  /** Remembers the calling tool's context for confirmations the session's pages trigger while it runs. */
  useContext(ctx: Confirmer | undefined, toolCallId?: string): void {
    this.#confirmer = ctx ?? NO_UI;
    this.#callId = toolCallId;
  }

  /** Whether a browser is open now, without opening one. */
  get isOpen(): boolean {
    return this.#session?.page.open === true;
  }

  async session(signal?: AbortSignal): Promise<BrowserSession> {
    if (this.#session?.page.open) return this.#session;
    // The page connection dropped: release what the old session owns before opening a new one.
    await this.close();
    return this.#start({ signal });
  }

  /** Drive a browser that is already open on a DevTools port, in its current page, instead of opening our own. */
  async attach(port: number | undefined, signal?: AbortSignal): Promise<BrowserSession> {
    if (!port) throw new Error('attach needs port (the DevTools port of an open browser).');
    await this.close();
    return this.#start({ port, reuseTab: true, signal });
  }

  async #start(options: { port?: number; reuseTab?: boolean; signal: AbortSignal | undefined }): Promise<BrowserSession> {
    const session = await openBrowser(process.env, { ...options, policy: this.#policy, onBlocked: (blocked) => this.#auditProxy(blocked) });
    if (this.#notice) session.warning = this.#notice;
    this.#notice = undefined;
    this.#session = session;
    return session;
  }

  /**
   * Ends the session. With `keepVisible` (session shutdown) a visible Chrome on the persistent profile stays open with
   * its tab, so the user can finish a login there and the next session reuses it.
   */
  async close(options: { keepVisible?: boolean } = {}): Promise<void> {
    const session = this.#session;
    this.#session = undefined;
    // What the browser session allowed and refused ends with it: a new Chrome starts with no approved host.
    this.#policy.revoke();
    this.#endAudit();
    if (!session) return;
    const keep = options.keepVisible === true && session.profile !== undefined;
    // A persistent-profile Chrome left open by an earlier run is quit by an explicit close, like one we launched.
    if (!keep && session.profile !== undefined && !session.chrome) await session.connection.send('Browser.close').catch(() => undefined);
    else if (!keep) await session.closeTab?.().catch(() => undefined);
    session.connection.close();
    session.consoleLog?.close();
    // Detached: Chrome runs in its own process group, so it outlives this process once nothing waits on it.
    if (keep) session.chrome?.process?.unref();
    else await session.chrome?.kill();
  }
}

export function registerBrowserTool(pi: ExtensionAPI, browser: BrowserTool): void {
  pi.registerTool(timedTool({
    name: 'browser',
    label: 'Browser',
    description:
      'Drive one Chrome session (JavaScript pages, forms, logins); headless unless visible mode is on. One action per call:\n' +
      'navigate(url), tab(url | index): snapshot = page text + numbered elements\n' +
      'snapshot(text?): wait for text first\n' +
      'click, hover(ref)\n' +
      'drag(ref, to | text)\n' +
      'type(ref, text, submit?), fill(fields), press(key)\n' +
      'wait(text, gone?), evaluate(expression)\n' +
      'screenshot(format?): jpeg (default) or png, shrunk to fit; result names the full-size file\n' +
      'dialog(accept?, text?), upload(ref, files), download(ref?)\n' +
      'tabs, console, info, close, attach(port: an open browser)\n' +
      'An element keeps its number while it stays on the page; a new document renumbers. Actions report what changed, so a fresh snapshot is rarely needed. A wait that times out fails with the current snapshot. Private/local addresses the agent opens (navigate, tab) and uploads outside the workspace need approval in manual and workspace permission modes. ' +
      'Requests the page itself starts (images, frames, scripts, fetch, redirects) to a private or local address are blocked without asking, except the address you opened. WebSockets: a launched Chrome blocks them through a filtering proxy (as it does DNS rebinding); in an attached or persistent-profile Chrome only the page and its requests are checked, so a page WebSocket to a private address is not stopped and is only reported.',
    promptSnippet: 'Inspect rendered pages and act on current element refs',
    promptGuidelines: ['Inspect the outcome before repeating a browser click or submission, and use existing user authorization for actions with side effects.'],
    // One shared page: parallel actions would race (a click against a navigation, stale element refs), so browser calls
    // run one at a time (`exclusive` below) while other tools in the same message still run beside them.
    parameters: PARAMETERS,
    renderCall: (args, theme, context) => browserCallHeader(args, theme, context),
    renderResult: (result, _options, theme, context) => browserResult(result, theme, context),
    async execute(id, params, signal, _onUpdate, ctx) {
      const { action } = params;
      const callCtx = { ...ctx, signal: signal ?? ctx.signal };
      browser.useContext(callCtx, id);
      try {
        if (isSessionAction(action)) return await SESSION_HANDLERS[action](browser, params, signal);
        const session = await browser.session(signal);
        if (session.dialog && PAGE_ACTIONS.has(action)) return pageResult(dialogNotice(session.dialog));
        return await PAGE_HANDLERS[action]({ session, page: session.page, params, signal, ctx: callCtx, browser, toolCallId: id });
      } catch (error) {
        throw visibleError(error);
      }
    },
  }, { exclusive: true }));
  pi.on('session_shutdown', async () => browser.close({ keepVisible: true }));
}

const SESSION_HANDLERS: Record<SessionAction, (browser: BrowserTool, params: BrowserParams, signal: AbortSignal | undefined) => Promise<ActionResult>> = {
  close: async (browser) => {
    await browser.close();
    return pageResult('Browser closed.');
  },
  info: async (browser, _params, signal) => {
    // A status query must not launch Chrome (a visible window under webLive).
    if (!browser.isOpen) return pageResult('No browser is open. navigate or attach starts one.');
    const { kind, port, headless, page, profile, dialog } = await browser.session(signal);
    const where = kind === 'own' ? `own Chrome (${headless ? 'headless' : 'visible'})` : kind === 'attached' ? 'attached window' : 'shared Chrome, own tab';
    const current = dialog ? dialogNotice(dialog) : String(await page.evaluate('location.href + " | " + document.title', signal));
    return pageResult(`${where}${profile ? ` · profile ${profile}` : ''} · DevTools port ${port} · ${current}`);
  },
  attach: async (browser, params, signal) => {
    const session = await browser.attach(params.port, signal);
    return pageResult(`Attached to the browser on DevTools port ${session.port}. Other CDP tools (for example the octocode-chrome-devtools skill's scripts) can use --port ${session.port}.\n\n${await snapshot(session, signal)}`);
  },
};

const isSessionAction = (action: BrowserAction): action is SessionAction => Object.hasOwn(SESSION_HANDLERS, action);

const hostOf = (url: string): string => {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
};

/** Leads the audit reason of a summary (a long reason may be cut for display, so the count goes first). */
const repeatNote = (repeats: number): string => (repeats > 0 ? `(${repeats + 1} times this browser session) ` : '');
