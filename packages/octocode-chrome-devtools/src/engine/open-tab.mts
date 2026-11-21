// The tab `open --url` makes on a browser that is already running. It follows the runner's
// own new-tab sequence (about:blank, stealth setup when enabled, then navigation), so the
// first document load already runs under the stealth overrides, and the tab is brought to
// the front.
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

/** One tab of a group reply: its url as requested, the Chrome target id and how its load ended. */
export type GroupTab = { url: string; targetId: string; status: string };

/** The stealth setup logs findings to stdout; open's stdout carries the result line only. */
export async function withStdoutOnStderr<T>(
  work: () => Promise<T>
): Promise<T> {
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
export async function loadGroupTab(
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
