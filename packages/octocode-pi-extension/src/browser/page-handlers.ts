import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { answerDialog, download, uploadFiles } from './actions.js';
import { checkUploadPaths } from './access.js';
import { click, drag, elementExpr, fillFields, hover, pressKey, refForText, settle, settleAfterLoad, typeInto, waitForText } from './input.js';
import { WAIT_DEFAULT_S, WAIT_MAX_S, type BrowserParams, type PageActionName } from './params.js';
import { screenshot } from './screenshot.js';
import { CONSOLE_KEEP, type BrowserSession } from './session-state.js';
import { snapshot } from './snapshot.js';
import { describeTabs, followNewTab, NAVIGATE_LOAD_MS, newTab, selectTab, tabIds, type Approval } from './tabs.js';
import type { Permission } from '../permission/ask.js';
import { formatDuration } from '../shared/format.js';
import { visibleControls } from '../shared/sanitize.js';
import { capOutputToFile } from '../shared/spill.js';
import { contentText, textResult, withTimeout } from '../shared/util.js';
import { imageLimits } from '../shared/vision.js';

export type ActionResult = { content: Array<{ type: 'text'; text: string } | { type: 'image'; data: string; mimeType: string }>; details: undefined };

/** What a page action needs of the browser tool: its permission, and the check on a page the agent opens (`BrowserTool.open`). */
interface PageOpener {
  readonly permission: Permission;
  open(url: string): Promise<Approval>;
}

interface PageAction {
  session: BrowserSession;
  page: BrowserSession['page'];
  params: BrowserParams;
  signal: AbortSignal | undefined;
  /** Read only by the actions that need it (upload checks); URL checks go through `browser`. */
  ctx: ExtensionContext;
  browser: PageOpener;
  toolCallId: string;
}

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
  return pageResult(withBlockedNote(session, typeof note === 'string' && note ? `${note}\n\n${after}` : after));
}

/** `text` led by a line naming the page request the policy refused, and one for what it could only report, since the last report; both are then forgotten. */
function withBlockedNote(session: BrowserSession, text: string): string {
  const { blocked, warning } = session;
  delete session.blocked;
  delete session.warning;
  return `${blocked ? `Blocked a page load: ${blocked}\n\n` : ''}${warning ? `Warning: ${warning}\n\n` : ''}${text}`;
}

/**
 * The page load the policy refused while a navigate ran, as a suffix for its failure. Read through a call: the network
 * handler sets `session.blocked` during the command, after `navigate` cleared it.
 */
const blockedDuring = (session: BrowserSession): string => (session.blocked ? ` (blocked ${session.blocked})` : '');

async function navigate({ session, page, params, signal, browser }: PageAction): Promise<ActionResult> {
  if (!params.url) throw new Error('navigate needs url');
  (await browser.open(params.url))(session.targetId);
  delete session.blocked;
  // The wait starts before the command so a fast load is not missed; `done` ends it when the command fails, and a
  // dialog (an alert() while loading holds the load event) ends it at once: the snapshot then reports the dialog.
  const done = new AbortController();
  const loaded = page.untilDialog(page.waitFor('Page.loadEventFired', NAVIGATE_LOAD_MS, signal ? AbortSignal.any([signal, done.signal]) : done.signal));
  // Handled here so a navigate that throws before awaiting it leaves no unhandled rejection; the await below still sees it.
  loaded.catch(() => undefined);
  try {
    const result = await page.send<{ errorText?: string; loaderId?: string }>('Page.navigate', { url: params.url }, signal);
    if (result.errorText) throw new Error(`Navigation failed: ${result.errorText}${blockedDuring(session)}`);
    // A same-document navigation (a #fragment) has no loader and fires no load event.
    if (result.loaderId) await loaded;
  } finally {
    done.abort();
  }
  // The load event fires before most apps fetch and render their data (prices, localized currency).
  // An open dialog (before or during the wait) leaves nothing to settle: the snapshot reports it.
  const load = session.dialog ? undefined : await page.untilDialog(settleAfterLoad(page, signal));
  const waited = params.text ? await waitUntil(session, params.text, params.timeout, signal) : '';
  const note =
    !load || load.settled || params.text ? ''
    : load.failed ? '\n\nNote: the page may still be loading (waiting for it to settle failed). Use wait (text) or snapshot again for content that loads late.'
    : '\n\nNote: the page was still changing 6s after loading; values may still update. Use wait (text) or snapshot again for content that loads late.';
  return pageResult(withBlockedNote(session, `${waited}${await snapshot(session, signal)}${note}`));
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
  // The body is already escaped; it is not run through a second, stripping pass (a CRLF in it stays a CRLF).
  return pageResult(`${head}\n\n${contentText(result.content)}`);
}

async function wait({ session, page, params, signal }: PageAction): Promise<ActionResult> {
  if (!params.text) throw new Error('wait needs text (the text to wait for).');
  const seconds = Math.min(Math.max(params.timeout ?? WAIT_DEFAULT_S, 1), WAIT_MAX_S);
  const took = await withSnapshotOnTimeout(session, signal, () => waitForText(page, params.text!, params.gone === true, seconds * 1000, signal));
  if (took === undefined) return pageResult(await snapshot(session, signal));
  return pageResult(`${JSON.stringify(params.text)} ${params.gone ? 'is gone' : 'appeared'} after ${formatDuration(took, true)}.\n\n${await snapshot(session, signal, { changesOnly: true })}`);
}

async function evaluate({ page, params, signal }: PageAction): Promise<ActionResult> {
  if (!params.expression) throw new Error('evaluate needs expression');
  const value = await page.evaluate(params.expression, signal);
  return pageResult(capOutputToFile(visibleControls(typeof value === 'string' ? value : JSON.stringify(value, null, 2) ?? 'undefined'), { label: 'browser_evaluate' }));
}

/** One handler per page action; each validates its own parameters. */
export const PAGE_HANDLERS: Record<PageActionName, (action: PageAction) => Promise<ActionResult>> = {
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
  tab: async ({ session, params, signal, browser }) => {
    if (params.url) await newTab(session, params.url, await browser.open(params.url), signal);
    else await selectTab(session, params.index, (target) => browser.open(target), signal);
    return pageResult(withBlockedNote(session, await snapshot(session, signal)));
  },
  dialog: async ({ session, params, signal }) => pageResult(await answerDialog(session, params.accept ?? true, params.text, signal)),
  upload: async ({ page, params, signal, ctx, browser, toolCallId }) => {
    const files = await checkUploadPaths(params.files ?? [], ctx, browser.permission, toolCallId);
    return pageResult(await uploadFiles(page, requireRef(params.ref), elementExpr(requireRef(params.ref)), files, signal));
  },
  download: async ({ session, page, params, signal }) => {
    const ref = params.ref;
    return pageResult(await download(session, ref === undefined ? undefined : async () => void (await page.untilDialog(click(page, ref, signal))), signal));
  },
  evaluate,
  screenshot: ({ page, params, signal, ctx }) => screenshot(page, params.format === 'png' ? 'png' : 'jpeg', signal, imageLimits(ctx.model)),
  console: async ({ session }) => {
    const { dropped, cleared } = session.consoleLost;
    // The buffer is a limited window; what it no longer holds is gone, so say how much instead of looking complete.
    const lost = [
      ...(dropped > 0 ? [`[${dropped} earlier console line${dropped === 1 ? '' : 's'} dropped from this view: only the last ${CONSOLE_KEEP} are kept]`] : []),
      ...(cleared > 0 ? [`[${cleared} line${cleared === 1 ? '' : 's'} from earlier tabs left this view when the tab changed]`] : []),
      // Dropped lines stay reachable in the log; say where, and what the log itself no longer holds.
      ...(dropped + cleared > 0 && session.consoleLog ? [`[${session.consoleLog.describe()}; search it or read it by line range]`] : []),
    ];
    const lines = session.console.length > 0 ? capOutputToFile(visibleControls(session.console.join('\n')), { label: 'browser_console' }) : '(no console messages)';
    return pageResult([...lost, lines].join('\n'));
  },
};

/** Page text (titles, snapshots, dialog messages, evaluate results) is untrusted: control and invisible characters show as visible escapes (`visibleControls`), never dropped. */
export function pageResult(text: string) {
  return textResult(visibleControls(text));
}

/** Waits for `text` before a snapshot (navigate, snapshot); a line saying how long it took. An open dialog (which blocks the page) skips it: the snapshot reports the dialog. */
async function waitUntil(session: BrowserSession, text: string, timeout: number | undefined, signal?: AbortSignal): Promise<string> {
  const seconds = Math.min(Math.max(timeout ?? WAIT_DEFAULT_S, 0), WAIT_MAX_S);
  const ms = session.dialog ? undefined : await withSnapshotOnTimeout(session, signal, () => waitForText(session.page, text, false, seconds * 1000, signal));
  return ms === undefined ? '' : `${JSON.stringify(text)} appeared after ${formatDuration(ms, true)}.\n\n`;
}

/** A timed-out wait still fails, but carries the current page so the model sees where it landed instead of retrying blind. */
async function withSnapshotOnTimeout<T>(session: BrowserSession, signal: AbortSignal | undefined, run: () => Promise<T>): Promise<T> {
  try {
    return await run();
  } catch (error) {
    if (signal?.aborted || !(error instanceof Error) || !error.message.startsWith('Timed out after')) throw error;
    // Bounded: a page that stopped answering must not stretch the failed wait.
    const current = await withTimeout(snapshot(session, signal), 3_000, 'snapshot').catch(() => '');
    throw new Error(visibleControls(current ? `${error.message}\n\nCurrent page:\n${current}` : error.message), { cause: error });
  }
}

function requireRef(ref: number | undefined): number {
  if (ref === undefined) throw new Error('This action needs ref (element number from snapshot).');
  return ref;
}
