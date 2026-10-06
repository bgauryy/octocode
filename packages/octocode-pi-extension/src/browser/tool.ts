import path from 'node:path';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { StringEnum } from '@earendil-works/pi-ai';
import { Type, type Static } from 'typebox';
import { answerDialog, dialogNotice, download, uploadFiles } from './actions.js';
import { browserCallHeader, browserResult } from './render.js';
import { openBrowser, type BrowserSession } from './cdp.js';
import { click, drag, elementExpr, fillFields, hover, pressKey, refForText, settle, settleAfterLoad, typeInto, waitForText } from './input.js';
import { snapshot } from './snapshot.js';
import { describeTabs, followNewTab, newTab, selectTab, tabIds } from './tabs.js';
import { withDialog } from '../shared/locks.js';
import { resultText, timedTool } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { capOutputToFile } from '../shared/spill.js';
import { textResult, withTimeout } from '../shared/util.js';
import { allowPrivate, isPrivateHost } from '../web/guard.js';

const ACTIONS = ['info', 'attach', 'navigate', 'snapshot', 'click', 'hover', 'drag', 'type', 'fill', 'press', 'wait', 'tabs', 'tab', 'evaluate', 'screenshot', 'console', 'dialog', 'upload', 'download', 'close'] as const;
/** Actions that talk to the page, so an open JavaScript dialog must be answered first. */
const PAGE_ACTIONS = new Set(['click', 'hover', 'drag', 'type', 'fill', 'press', 'wait', 'evaluate', 'upload', 'download']);
/** Default and longest `wait`, in seconds. */
const WAIT_DEFAULT_S = 10;
const WAIT_MAX_S = 60;

export class BrowserTool {
  #session: BrowserSession | undefined;
  /** Private hosts (`host:port`) the user allowed this session. */
  readonly allowedHosts = new Set<string>();

  /** Whether a browser is open now, without opening one. */
  get isOpen(): boolean {
    return this.#session?.page.open === true;
  }

  async session(): Promise<BrowserSession> {
    if (this.#session?.page.open) return this.#session;
    // The page connection dropped: release what the old session owns before opening a new one.
    await this.close();
    this.#session = await openBrowser();
    return this.#session;
  }

  /** Drive a browser that is already open on a DevTools port, in its current page, instead of opening our own. */
  async attach(port: number | undefined): Promise<BrowserSession> {
    if (!port) throw new Error('attach needs port (the DevTools port of an open browser).');
    await this.close();
    this.#session = await openBrowser(process.env, { port, reuseTab: true });
    return this.#session;
  }

  /**
   * Ends the session. With `keepVisible` (session shutdown) a visible Chrome on the persistent profile stays open with
   * its tab, so the user can finish a login there and the next session reuses it.
   */
  async close(options: { keepVisible?: boolean } = {}): Promise<void> {
    const session = this.#session;
    this.#session = undefined;
    if (!session) return;
    const keep = options.keepVisible === true && session.profile !== undefined;
    // A persistent-profile Chrome left open by an earlier run is quit by an explicit close, like one we launched.
    if (!keep && session.profile !== undefined && !session.chrome) await session.page.send('Browser.close').catch(() => undefined);
    else if (!keep) await session.closeTab?.().catch(() => undefined);
    session.page.close();
    // Detached: Chrome runs in its own process group, so it outlives this process once nothing waits on it.
    if (keep) session.chrome?.process?.unref();
    else await session.chrome?.kill();
  }
}

const PARAMETERS = Type.Object({
  action: StringEnum(ACTIONS),
  url: Type.Optional(Type.String({ description: 'navigate: URL; waits until the page is quiet (up to 6s); tab: URL to open' })),
  port: Type.Optional(Type.Integer({ description: 'attach: DevTools port of an open browser' })),
  ref: Type.Optional(Type.Integer({ description: 'Element number from snapshot (open shadow DOM and same-origin frames included)' })),
  to: Type.Optional(Type.Integer({ description: 'drag: element number to drop on (or give text instead)' })),
  fields: Type.Optional(Type.Array(Type.Object({ ref: Type.Integer(), value: Type.String() }), { maxItems: 50, description: 'fill: fields to set' })),
  index: Type.Optional(Type.Integer({ minimum: 1, description: 'tab: tab number from tabs' })),
  gone: Type.Optional(Type.Boolean({ description: 'wait: until text disappears' })),
  timeout: Type.Optional(Type.Number({ description: `wait (and navigate/snapshot with text): seconds (default ${WAIT_DEFAULT_S}, max ${WAIT_MAX_S})` })),
  text: Type.Optional(Type.String({ description: 'Meaning depends on action. type: the value to enter (replaces the current value; for a <select>, the option label to pick). wait: the text to wait for (or, with gone, to disappear). navigate/snapshot: optional text to wait for before returning. drag: visible text of the drop zone, instead of to. dialog: the reply to a prompt() dialog.' })),
  accept: Type.Optional(Type.Boolean({ description: 'dialog: accept (default) or false to dismiss' })),
  files: Type.Optional(Type.Array(Type.String(), { description: 'upload: file paths' })),
  submit: Type.Optional(Type.Boolean({ description: 'Press Enter after typing' })),
  key: Type.Optional(Type.String({ description: 'press: key or combo, e.g. Enter, Escape, Control+A, Shift+Tab' })),
  expression: Type.Optional(Type.String({ description: 'JavaScript expression for evaluate' })),
});
type BrowserParams = Static<typeof PARAMETERS>;

export function registerBrowserTool(pi: ExtensionAPI, browser: BrowserTool): void {
  pi.registerTool(timedTool({
    name: 'browser',
    label: 'Browser',
    description:
      'Inspect and interact with one Chrome session for JavaScript pages, forms and authenticated flows; headless unless visible mode is enabled. ' +
      'Actions: navigate(url) and tab(url | index) return a snapshot: page text plus numbered elements; snapshot; click/hover(ref); drag(ref, to | text); type(ref, text, submit?); fill(fields); press(key); wait(text, gone?); tabs; evaluate(expression); screenshot; console; dialog(accept?, text?); upload(ref, files); download(ref?); info; attach(port: an open browser); close. ' +
      'An element keeps its number while it stays on the page; a new document renumbers. Other actions report what changed, so a fresh snapshot is rarely needed. A wait that times out fails with the current snapshot. Private/local addresses require confirmation unless explicitly enabled in configuration; uploads outside the workspace require interactive confirmation.',
    promptSnippet: 'Inspect rendered pages and act on current element refs',
    promptGuidelines: ['Use web for static public text. Use browser for interaction; inspect the outcome before repeating a click or submission, and use existing user authorization for actions with side effects.'],
    // One shared page: parallel actions would race (a click against a navigation, stale element refs), so browser calls
    // run one at a time (`exclusive` below) while other tools in the same message still run beside them.
    parameters: PARAMETERS,
    renderCall: (args, theme, context) => browserCallHeader(args, theme, context),
    renderResult: (result, _options, theme, context) => browserResult(result, theme, context),
    async execute(_id, params, signal, _onUpdate, ctx) {
      const { action } = params;
      if (isSessionAction(action)) return SESSION_HANDLERS[action](browser, params, signal);
      const session = await browser.session();
      if (session.dialog && PAGE_ACTIONS.has(action)) return pageResult(dialogNotice(session.dialog));
      return PAGE_HANDLERS[action]({ session, page: session.page, params, signal, ctx, browser });
    },
  }, { exclusive: true }));
  pi.on('session_shutdown', async () => browser.close({ keepVisible: true }));
}

type ActionResult = { content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>; details: undefined };
type BrowserAction = (typeof ACTIONS)[number];
/** Actions that manage the session itself rather than act on its page. */
type SessionAction = 'close' | 'info' | 'attach';
type PageActionName = Exclude<BrowserAction, SessionAction>;

interface PageAction {
  session: BrowserSession;
  page: BrowserSession['page'];
  params: BrowserParams;
  signal: AbortSignal | undefined;
  /** Read only by the actions that need it (URL and upload checks, download paths). */
  ctx: ExtensionContext;
  browser: BrowserTool;
}

const SESSION_HANDLERS: Record<SessionAction, (browser: BrowserTool, params: BrowserParams, signal: AbortSignal | undefined) => Promise<ActionResult>> = {
  close: async (browser) => {
    await browser.close();
    return pageResult('Browser closed.');
  },
  info: async (browser, _params, signal) => {
    // A status query must not launch Chrome (a visible window under webLive).
    if (!browser.isOpen) return pageResult('No browser is open. navigate or attach starts one.');
    const { kind, port, headless, page, profile, dialog } = await browser.session();
    const where = kind === 'own' ? `own Chrome (${headless ? 'headless' : 'visible'})` : kind === 'attached' ? 'attached window' : 'shared Chrome, own tab';
    const current = dialog ? dialogNotice(dialog) : String(await page.evaluate('location.href + " | " + document.title', signal));
    return pageResult(`${where}${profile ? ` · profile ${profile}` : ''} · DevTools port ${port} · ${current}`);
  },
  attach: async (browser, params, signal) => {
    const session = await browser.attach(params.port);
    return pageResult(`Attached to the browser on DevTools port ${session.port}. Other CDP tools (for example the octocode-chrome-devtools skill's scripts) can use --port ${session.port}.\n\n${await snapshot(session, signal)}`);
  },
};

const isSessionAction = (action: BrowserAction): action is SessionAction => Object.hasOwn(SESSION_HANDLERS, action);

/**
 * Runs page input that may open a dialog (the snapshot after it then reports the dialog instead of hanging), waits
 * for the page to settle, and follows a tab the input opened.
 */
async function input({ session, page, signal }: PageAction, start: () => Promise<unknown>): Promise<ActionResult> {
  const before = await tabIds(session);
  // Started only now, so a failure cannot go unhandled while the tab list is read.
  const note = await page.untilDialog(start());
  if (!session.dialog) await page.untilDialog(settle(page, signal));
  const opened = session.dialog ? undefined : await followNewTab(session, before, signal);
  const after = opened ? `This action opened a new tab; now driving it (tabs lists all).\n\n${await snapshot(session, signal)}` : await snapshot(session, signal, { changesOnly: true });
  return pageResult(typeof note === 'string' && note ? `${note}\n\n${after}` : after);
}

/**
 * Refuses non-http(s) URLs, and private or local hosts (loopback, LAN, cloud metadata) unless the user confirms them
 * once per session or OCTOCODE_WEB_ALLOW_PRIVATE=1: page content steers the agent, so it must not reach internal
 * services the `web` tool deliberately blocks.
 */
export async function checkBrowserUrl(raw: string, ctx: Pick<ExtensionContext, 'hasUI' | 'ui'>, allowed: Set<string>): Promise<void> {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(`Not a URL: ${raw}`);
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error(`The browser opens http(s) URLs only, not ${url.protocol}`);
  if (allowPrivate() || allowed.has(url.host) || !(await isPrivateHost(url))) return;
  if (ctx.hasUI && (await withDialog(() => ctx.ui.confirm('Open a private address?', `The browser wants to open ${url.host}, a local or private network address.`)))) {
    allowed.add(url.host);
    return;
  }
  throw new Error(`Blocked private or local address: ${url.host}. It needs the user's confirmation in an interactive session, or OCTOCODE_WEB_ALLOW_PRIVATE=1.`);
}

/** Upload paths resolved against `cwd`; paths outside it need the user's confirmation. */
export async function checkUploadPaths(files: readonly string[], ctx: Pick<ExtensionContext, 'cwd' | 'hasUI' | 'ui'>): Promise<void> {
  const inside = (file: string) => {
    const relative = path.relative(ctx.cwd, file);
    return relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative);
  };
  const outside = files.map((file) => path.resolve(ctx.cwd, file)).filter((file) => !inside(file));
  if (outside.length === 0) return;
  if (ctx.hasUI && (await withDialog(() => ctx.ui.confirm('Upload files from outside the workspace?', outside.join('\n'))))) return;
  throw new Error(`Refused to upload files outside the workspace without the user's confirmation: ${outside.join(', ')}`);
}

async function navigate({ session, page, params, signal, ctx, browser }: PageAction): Promise<ActionResult> {
  if (!params.url) throw new Error('navigate needs url');
  await checkBrowserUrl(params.url, ctx, browser.allowedHosts);
  // The wait starts before the command so a fast load is not missed; `done` ends it when the command fails, and a
  // dialog (an alert() while loading holds the load event) ends it at once: the snapshot then reports the dialog.
  const done = new AbortController();
  const loaded = page.untilDialog(page.waitFor('Page.loadEventFired', 20_000, signal ? AbortSignal.any([signal, done.signal]) : done.signal));
  loaded.catch(() => undefined);
  try {
    const result = await page.send<{ errorText?: string; loaderId?: string }>('Page.navigate', { url: params.url }, signal);
    if (result.errorText) throw new Error(`Navigation failed: ${result.errorText}`);
    // A same-document navigation (a #fragment) has no loader and fires no load event.
    if (result.loaderId) await loaded;
  } finally {
    done.abort();
  }
  // The load event fires before most apps fetch and render their data (prices, localized currency).
  const settled = session.dialog ? true : ((await page.untilDialog(settleAfterLoad(page, signal)))?.settled ?? true);
  const waited = params.text ? await waitUntil(session, params.text, params.timeout, signal) : '';
  const note = settled || params.text ? '' : '\n\nNote: the page was still changing 6s after loading; values may still update. Use wait (text) or snapshot again for content that loads late.';
  return pageResult(`${waited}${await snapshot(session, signal)}${note}`);
}

async function dragAction(action: PageAction): Promise<ActionResult> {
  const { page, params, signal } = action;
  const from = requireRef(params.ref);
  if (params.to === undefined && !params.text) throw new Error('drag needs to (the element number to drop on), or text (visible text of the drop zone).');
  const to = params.to ?? (await refForText(page, params.text!, signal));
  return input(action, () => drag(page, from, to, signal));
}

async function fill(action: PageAction): Promise<ActionResult> {
  const { page, params, signal } = action;
  if (!params.fields?.length) throw new Error('fill needs fields: [{ ref, value }].');
  const fields = params.fields;
  const failures: string[] = [];
  const result = await input(action, () => fillFields(page, fields, signal).then((failed) => void failures.push(...failed)));
  if (failures.length === fields.length) throw new Error(`No field was filled:\n${failures.join('\n')}`);
  const head = `Filled ${fields.length - failures.length} of ${fields.length} field(s).${failures.length ? `\nFailed:\n${failures.join('\n')}` : ''}`;
  return pageResult(`${head}\n\n${resultText(result)}`);
}

async function wait({ session, page, params, signal }: PageAction): Promise<ActionResult> {
  if (!params.text) throw new Error('wait needs text (the text to wait for).');
  const seconds = Math.min(Math.max(params.timeout ?? WAIT_DEFAULT_S, 1), WAIT_MAX_S);
  const took = await withSnapshotOnTimeout(session, signal, () => waitForText(page, params.text!, params.gone === true, seconds * 1000, signal));
  if (took === undefined) return pageResult(await snapshot(session, signal));
  return pageResult(`${JSON.stringify(params.text)} ${params.gone ? 'is gone' : 'appeared'} after ${(took / 1000).toFixed(1)}s.\n\n${await snapshot(session, signal, { changesOnly: true })}`);
}

async function evaluate({ page, params, signal }: PageAction): Promise<ActionResult> {
  if (!params.expression) throw new Error('evaluate needs expression');
  const value = await page.evaluate(params.expression, signal);
  return pageResult(capOutputToFile(sanitizeTerminalText(typeof value === 'string' ? value : JSON.stringify(value, null, 2) ?? 'undefined'), { label: 'browser_evaluate' }));
}

/** One handler per page action; each validates its own parameters. */
const PAGE_HANDLERS: Record<PageActionName, (action: PageAction) => Promise<ActionResult>> = {
  navigate,
  snapshot: async ({ session, params, signal }) => {
    const waited = params.text ? await waitUntil(session, params.text, params.timeout, signal) : '';
    return pageResult(`${waited}${await snapshot(session, signal)}`);
  },
  click: (action) => input(action, () => click(action.page, requireRef(action.params.ref), action.signal)),
  hover: (action) => input(action, () => hover(action.page, requireRef(action.params.ref), action.signal)),
  drag: dragAction,
  // Refusals (no such element or option, not a text field) throw before any input, so nothing lands elsewhere.
  type: (action) => {
    if (action.params.text === undefined) throw new Error('type needs text (the value to enter, or the <select> option label).');
    return input(action, () => typeInto(action.page, requireRef(action.params.ref), action.params.text!, action.params.submit === true, action.signal));
  },
  fill,
  press: (action) => {
    // No default key: a forgotten key must not press Enter and submit a form.
    if (!action.params.key) throw new Error('press needs key (e.g. Enter, Escape, Control+A).');
    return input(action, () => pressKey(action.page, action.params.key!, action.signal));
  },
  wait,
  tabs: async ({ session }) => pageResult(await describeTabs(session)),
  tab: async ({ session, params, signal, ctx, browser }) => {
    if (params.url) await checkBrowserUrl(params.url, ctx, browser.allowedHosts);
    if (params.url) await newTab(session, params.url, signal);
    else await selectTab(session, params.index, signal);
    return pageResult(await snapshot(session, signal));
  },
  dialog: async ({ session, params, signal }) => pageResult(await answerDialog(session, params.accept ?? true, params.text, signal)),
  upload: async ({ page, params, signal, ctx }) => {
    await checkUploadPaths(params.files ?? [], ctx);
    return pageResult(await uploadFiles(page, requireRef(params.ref), elementExpr(requireRef(params.ref)), params.files, ctx.cwd, signal));
  },
  download: async ({ session, page, params, signal, ctx }) => {
    const ref = params.ref;
    return pageResult(await download(session, ctx.cwd, ref === undefined ? undefined : async () => void (await page.untilDialog(click(page, ref, signal))), signal));
  },
  evaluate,
  screenshot: async ({ page, signal }) => {
    const shot = await page.send<{ data: string }>('Page.captureScreenshot', { format: 'png' }, signal);
    return { content: [{ type: 'image', data: shot.data, mimeType: 'image/png' }], details: undefined };
  },
  console: async ({ session }) => pageResult(session.console.length > 0 ? capOutputToFile(sanitizeTerminalText(session.console.join('\n')), { label: 'browser_console' }) : '(no console messages)'),
};

/** Page text (titles, snapshots, dialog messages, evaluate results) is untrusted: strip escape sequences before the model sees it. */
function pageResult(text: string) {
  return textResult(sanitizeTerminalText(text));
}

/** Waits for `text` before a snapshot (navigate, snapshot); a line saying how long it took. An open dialog (which blocks the page) skips it: the snapshot reports the dialog. */
async function waitUntil(session: BrowserSession, text: string, timeout: number | undefined, signal?: AbortSignal): Promise<string> {
  const seconds = Math.min(Math.max(timeout ?? WAIT_DEFAULT_S, 0), WAIT_MAX_S);
  const ms = session.dialog ? undefined : await withSnapshotOnTimeout(session, signal, () => waitForText(session.page, text, false, seconds * 1000, signal));
  return ms === undefined ? '' : `${JSON.stringify(text)} appeared after ${(ms / 1000).toFixed(1)}s.\n\n`;
}

/** A timed-out wait still fails, but carries the current page so the model sees where it landed instead of retrying blind. */
async function withSnapshotOnTimeout<T>(session: BrowserSession, signal: AbortSignal | undefined, run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    if (signal?.aborted || !(error instanceof Error) || !error.message.startsWith('Timed out after')) throw error;
    // Bounded: a page that stopped answering must not stretch the failed wait.
    const current = await withTimeout(snapshot(session, signal), 3_000, 'snapshot').catch(() => '');
    throw new Error(sanitizeTerminalText(current ? `${error.message}\n\nCurrent page:\n${current}` : error.message));
  }
}

function requireRef(ref: number | undefined): number {
  if (ref === undefined) throw new Error('This action needs ref (element number from snapshot).');
  return ref;
}
