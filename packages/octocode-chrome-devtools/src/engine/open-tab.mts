// The tab `open --url` makes on a browser that is already running. It follows the runner's
// own new-tab sequence (about:blank, stealth setup when enabled, then navigation), so the
// first document load already runs under the stealth overrides, and the tab is brought to
// the front.
import { randomUUID } from 'crypto';
import { connectCDP } from './cdp-connection.mjs';
import { errorMessage } from './error-message.mjs';
import { isAboutOrDataUrl } from './mandatory-stealth.mjs';
import { originViolation } from './session-meta.mjs';
import {
  chromeEndpoint,
  confirmTabLoaded,
  newTabTarget,
  stealthThenNavigate,
} from './cdp-runner-targets.mjs';

type Note = (line: string) => void;

/** One tab of a group: its url as requested, the Chrome target id and how its load ended. */
export type GroupTab = { url: string; targetId: string; status: string };

/** The stealth setup logs findings to stdout; open's stdout carries the result line only. */
async function withStdoutOnStderr<T>(work: () => Promise<T>): Promise<T> {
  // oxlint-disable-next-line eslint/no-console -- stdout is open's result protocol
  const stdout = console.log;
  // oxlint-disable-next-line eslint/no-console -- stdout is open's result protocol
  console.log = console.error;
  try {
    return await work();
  } finally {
    // oxlint-disable-next-line eslint/no-console -- restores stdout
    console.log = stdout;
  }
}

/** Opens `url` in a new tab on `port`, makes it the active tab and returns its target id. */
export async function openSelectedTab(
  port: string,
  url: string,
  note: Note,
  allowOrigins?: readonly string[]
): Promise<string> {
  const chrome = chromeEndpoint(port);
  const tab = await newTabTarget(chrome, url, note);
  const id = tab.openedTabId;
  if (!id || !tab.targetWsUrl) throw new Error('Chrome returned no tab');
  // The stealth setup logs findings to stdout; open's stdout carries the result line only.
  // oxlint-disable-next-line eslint/no-console -- stdout is open's result protocol
  const stdout = console.log;
  // oxlint-disable-next-line eslint/no-console -- stdout is open's result protocol
  console.log = console.error;
  let cdp;
  try {
    cdp = await connectCDP(tab.targetWsUrl, { targetInfo: tab.targetInfo });
    await confirmTabLoaded(cdp, note);
    await chrome.activate(id);
    const reached = await stealthThenNavigate(
      cdp,
      tab.pendingNavigate ?? null,
      {
        log: note,
        note,
      }
    );
    // A redirect may have left the allowlist; the tab is closed below with this error.
    const violation = reached && originViolation(reached, allowOrigins);
    if (violation)
      throw new Error(`The page redirected to ${reached}. ${violation}`);
    return id;
  } catch (error) {
    // A tab that never loaded its URL would only mislead later commands.
    await chrome.close(id);
    throw error;
  } finally {
    cdp?.close();
    // oxlint-disable-next-line eslint/no-console -- restores stdout
    console.log = stdout;
  }
}

async function waitForPageTarget(
  chrome: ReturnType<typeof chromeEndpoint>,
  id: string
) {
  for (let attempt = 0; attempt < 30; attempt++) {
    const row = (await chrome.targets()).find(t => t.id === id);
    if (row?.webSocketDebuggerUrl) return row;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  throw new Error(`Chrome did not list tab ${id}`);
}

/** Loads one group tab and says how it ended: loaded, timeout, or error text. */
async function loadGroupTab(
  chrome: ReturnType<typeof chromeEndpoint>,
  id: string,
  url: string,
  note: Note,
  allowOrigins?: readonly string[]
): Promise<string> {
  const row = await waitForPageTarget(chrome, id);
  const cdp = await connectCDP(row.webSocketDebuggerUrl!, { targetInfo: row });
  let timedOut = false;
  try {
    await confirmTabLoaded(cdp, note);
    const reached = await stealthThenNavigate(
      cdp,
      isAboutOrDataUrl(url) ? null : url,
      {
        log: line => {
          if (line.includes('PAGE_LOAD_TIMEOUT')) timedOut = true;
          note(line);
        },
        note,
      }
    );
    const violation = reached && originViolation(reached, allowOrigins);
    if (violation) return `error: redirected to ${reached}. ${violation}`;
    return timedOut ? 'timeout' : 'loaded';
  } catch (error) {
    return `error: ${errorMessage(error)}`;
  } finally {
    cdp.close();
  }
}

/**
 * One tab per url through Target.createTarget, in order (optionally all in one new isolated
 * browser context that `disposeBrowserContexts` removes). Each tab opens on about:blank and
 * navigates after the stealth setup, like every other new tab, then reports its own status.
 */
export async function openTabGroup(
  port: string,
  urls: string[],
  options: {
    background?: boolean;
    newWindow?: boolean;
    isolated?: boolean;
    allowOrigins?: readonly string[];
  },
  note: Note
): Promise<{ tabs: GroupTab[]; contextId?: string }> {
  const chrome = chromeEndpoint(port);
  const version = await chrome.version();
  const browser = await connectCDP(version.webSocketDebuggerUrl, {
    targetInfo: { type: 'browser' },
  });
  const tabs: GroupTab[] = [];
  let contextId: string | undefined;
  try {
    if (options.isolated)
      contextId = (await browser.send('Target.createBrowserContext', {}))
        .browserContextId;
    await withStdoutOnStderr(async () => {
      for (const url of urls) {
        const created = await browser.send('Target.createTarget', {
          url: isAboutOrDataUrl(url) ? url : 'about:blank',
          ...(contextId ? { browserContextId: contextId } : {}),
          ...(options.newWindow ? { newWindow: true } : {}),
          ...(options.background ? { background: true } : {}),
        });
        const targetId: string = created.targetId;
        tabs.push({
          url,
          targetId,
          status: await loadGroupTab(
            chrome,
            targetId,
            url,
            note,
            options.allowOrigins
          ).catch(error => `error: ${errorMessage(error)}`),
        });
      }
    });
  } catch (error) {
    // A half-made group would leave unnamed tabs and a context nobody disposes.
    if (contextId)
      await browser
        .send('Target.disposeBrowserContext', { browserContextId: contextId })
        .catch(() => {});
    else
      for (const tab of tabs)
        await browser
          .send('Target.closeTarget', { targetId: tab.targetId })
          .catch(() => {});
    throw error;
  } finally {
    browser.close();
  }
  return { tabs, contextId };
}

/** Disposes isolated browser contexts (and so their tabs); returns how many Chrome accepted. */
export async function disposeBrowserContexts(
  port: string,
  contextIds: readonly string[]
): Promise<number> {
  if (!contextIds.length) return 0;
  const version = await chromeEndpoint(port).version();
  const browser = await connectCDP(version.webSocketDebuggerUrl, {
    targetInfo: { type: 'browser' },
  });
  let disposed = 0;
  try {
    for (const browserContextId of contextIds)
      await browser
        .send('Target.disposeBrowserContext', { browserContextId })
        .then(() => disposed++)
        .catch(() => {});
  } finally {
    browser.close();
  }
  return disposed;
}

/** `g-` plus a short random id: the name of a group opened without one. */
export const generatedGroupName = () => `g-${randomUUID().slice(0, 6)}`;
