// The tab `open --url` makes on a browser that is already running. It follows the runner's
// own new-tab sequence (about:blank, stealth setup when enabled, then navigation), so the
// first document load already runs under the stealth overrides, and the tab is brought to
// the front.
import { connectCDP } from './cdp-connection.mjs';
import {
  chromeEndpoint,
  confirmTabLoaded,
  newTabTarget,
  stealthThenNavigate,
} from './cdp-runner-targets.mjs';

/** Opens `url` in a new tab on `port`, makes it the active tab and returns its target id. */
export async function openSelectedTab(
  port: string,
  url: string,
  note: (line: string) => void
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
    await stealthThenNavigate(cdp, tab.pendingNavigate ?? null, {
      log: note,
      note,
    });
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
