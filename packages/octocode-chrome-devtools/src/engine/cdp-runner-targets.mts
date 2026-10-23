// Which Chrome target a runner attaches to: the DevTools HTTP endpoint, target selection by
// flag (browser, new tab, id, URL, type, first page), the target inventory and the first navigation.
import { join } from 'path';
import { cdpHttp, cdpRequest } from './engine-io.mjs';
import { isAboutOrDataUrl, stealthEnabled } from './mandatory-stealth.mjs';
import {
  readJson,
  writeJson,
  writeTargetSnapshot,
} from './cdp-runner-session.mjs';
import { pageOwner } from './target-owner.mjs';

/** A /json target row; the runner attaches to its webSocketDebuggerUrl. */
type ChromeTarget = {
  id: string;
  type: string;
  url: string;
  title: string;
  webSocketDebuggerUrl?: string;
};

/** The selected target, or the message to print before exiting. */
type Selection = {
  targetWsUrl?: string;
  targetInfo?: ChromeTarget;
  openedTabId?: string;
  pendingNavigate?: string | null;
  error?: string;
};

/** Chrome's DevTools HTTP endpoint on one port. */
export function chromeEndpoint(port) {
  return {
    version: () => cdpHttp(port, '/json/version'),
    targets: (): Promise<ChromeTarget[]> => cdpHttp(port, '/json'),
    open: url =>
      cdpHttp(port, `/json/new?${encodeURIComponent(url)}`, { method: 'PUT' }),
    // Chrome answers /json/activate with the plain text `Target activated` (labelled JSON), so only the status counts.
    activate: async id => {
      await cdpRequest(port, `/json/activate/${id}`);
    },
    close: async id => {
      try {
        await cdpRequest(port, `/json/close/${id}`, { timeoutMs: 3000 });
        return true;
      } catch {
        return false;
      }
    },
    protocol: () => cdpHttp(port, '/json/protocol'),
  };
}

function uniqueTarget(matches, strict) {
  if (strict && matches.length > 1)
    throw new Error(
      `Ambiguous target: ${matches.length} matches. Run targets and select --target <id>.`
    );
  return matches[0];
}

const attach = (t: ChromeTarget): Selection => ({
  targetWsUrl: t.webSocketDebuggerUrl,
  targetInfo: t,
});

async function browserTarget(chrome, sel): Promise<Selection> {
  const pageSelection =
    sel.newTab || sel.targetId || sel.targetUrl || sel.targetType;
  if (pageSelection || stealthEnabled())
    throw new Error(
      '--browser cannot combine with page selection or --stealth'
    );
  const version = await chrome.version();
  return {
    targetWsUrl: version.webSocketDebuggerUrl,
    targetInfo: {
      id: 'browser',
      type: 'browser',
      title: version.Browser,
      url: '',
    },
  };
}

// A non-blank URL opens on about:blank first and navigates after stealth is applied.
async function newTabTarget(chrome, tabUrl, note): Promise<Selection> {
  const openUrl = !isAboutOrDataUrl(tabUrl) ? 'about:blank' : tabUrl;
  const pendingNavigate = openUrl !== tabUrl ? tabUrl : null;
  const tab = await chrome.open(openUrl);
  note(
    `[CDP_RUNNER] Opened new tab (${tab.id}) -> ${openUrl}${pendingNavigate ? ` (pending ${pendingNavigate})` : ''}`
  );
  return {
    targetWsUrl: tab.webSocketDebuggerUrl,
    targetInfo: { id: tab.id, url: tab.url, title: tab.title, type: tab.type },
    openedTabId: tab.id,
    pendingNavigate,
  };
}

async function targetById(chrome, id): Promise<Selection> {
  const t = (await chrome.targets()).find(x => x.id === id);
  if (!t) return { error: `[CDP_RUNNER] Target ${id} not found` };
  await chrome.activate(id).catch(() => {});
  return attach(t);
}

async function targetByUrl(chrome, sel, note): Promise<Selection> {
  const targets = await chrome.targets();
  const pool = targets.filter(t => t.type === (sel.targetType || 'page'));
  const t = uniqueTarget(
    pool.filter(x => x.url && x.url.includes(sel.targetUrl)),
    sel.strict
  );
  if (!t) {
    const available = targets.map(x => `  [${x.type}] ${x.url}`).join('\n');
    return {
      error: `[CDP_RUNNER] No target URL matching "${sel.targetUrl}". Available targets:\n${available}`,
    };
  }
  note(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);
  return attach(t);
}

async function targetByType(chrome, sel, note): Promise<Selection> {
  const targets = await chrome.targets();
  const t = uniqueTarget(
    targets.filter(x => x.type === sel.targetType),
    sel.strict
  );
  if (!t) {
    const available = [...new Set<string>(targets.map(x => x.type))].join(', ');
    return {
      error: `[CDP_RUNNER] No target of type "${sel.targetType}". Available types: ${available}`,
    };
  }
  note(`[CDP_RUNNER] Matched target [${t.type}]: ${t.url}`);
  return attach(t);
}

async function firstPage(chrome, sel, note): Promise<Selection> {
  const pages = (await chrome.targets()).filter(t => t.type === 'page');
  if (pages.length === 0)
    return {
      error:
        '[CDP_RUNNER] No page targets. Open a tab in Chrome first, or use --new-tab <url>',
    };
  const t = uniqueTarget(pages, sel.strict);
  note(`[CDP_RUNNER] Using tab: ${t.url}`);
  return attach(t);
}

/**
 * The target to attach to: { targetWsUrl, targetInfo, openedTabId?, pendingNavigate? }, or
 * { error } to print before exiting. An ambiguous match under sel.strict throws.
 */
export function selectTarget(chrome, sel, note): Promise<Selection> {
  if (sel.browser) return browserTarget(chrome, sel);
  if (sel.newTab) return newTabTarget(chrome, sel.newTab, note);
  if (sel.targetId) return targetById(chrome, sel.targetId);
  if (sel.targetUrl) return targetByUrl(chrome, sel, note);
  if (sel.targetType) return targetByType(chrome, sel, note);
  return firstPage(chrome, sel, note);
}

/** How the target was selected, as recorded in the session metadata. */
export function selectionVia(sel) {
  if (sel.newTab) return 'new-tab';
  if (sel.targetId) return 'target';
  if (sel.targetUrl) return 'target-url';
  if (sel.targetType) return 'target-type';
  return 'first-page';
}

function targetsContinuation(file, count) {
  return {
    count,
    artifact: file,
    next: {
      continue: {
        command: process.execPath,
        args: [join(import.meta.dirname, 'evidence-query.mjs'), '--file', file],
      },
    },
  };
}

/**
 * cdp.foreground(): brings the attached page to the front. An iframe target activates its
 * top-level page owner through a short-lived browser session.
 */
export function attachForeground(cdp, targetInfo, browserSession) {
  cdp.foreground = async () => {
    if (targetInfo.type !== 'iframe') return cdp.send('Page.bringToFront');
    const browser = await browserSession();
    try {
      const { targetInfos } = await browser.send('Target.getTargets');
      const owner = pageOwner(
        targetInfos,
        targetInfos.find(info => info.targetId === targetInfo.id)
      );
      if (!owner)
        throw new Error(
          'Cannot resolve iframe parent page from Target.getTargets parentId'
        );
      await browser.send('Target.activateTarget', { targetId: owner.targetId });
    } finally {
      browser.close();
    }
    return undefined;
  };
}

/** Records and prints the target inventory; a large one is written to a file with a query continuation. */
export async function listTargets(chrome, { port, browser, session }, log) {
  const nowIso = new Date().toISOString();
  const targets = await chrome.targets();
  writeTargetSnapshot(session.targetSnapshotFile, {
    capturedAt: nowIso,
    port,
    targets,
  });
  writeJson(session.sessionMetaFile, {
    ...readJson(session.sessionMetaFile, {}),
    port,
    browser,
    lastListedTargetsAt: nowIso,
    updatedAt: nowIso,
  });
  const rows = targets.map(t => ({
    id: t.id,
    type: t.type,
    url: t.url,
    title: t.title,
  }));
  const encoded = JSON.stringify(rows, null, 2);
  // Small inventories stay convenient; large ones retain an immutable source
  // and a lossless query continuation instead of overflowing agent context.
  if (Buffer.byteLength(encoded) <= 18000) {
    log(encoded);
    return;
  }
  const file = join(
    session.sessionMetaDir,
    `targets-${Date.now()}-${Math.random().toString(36).slice(2)}.json`
  );
  writeJson(file, rows);
  log(JSON.stringify(targetsContinuation(file, rows.length)));
}

async function loadOrTimeout(cdp, url, log) {
  let onLoad, timer;
  const loaded = new Promise(resolve => {
    onLoad = resolve;
    cdp.on('Page.loadEventFired', onLoad);
  });
  try {
    const navigation = await cdp.send('Page.navigate', { url });
    if (navigation.errorText)
      throw new Error(`Navigation failed: ${navigation.errorText}`);
    const completed = await Promise.race([
      loaded.then(() => true),
      new Promise(resolve => {
        timer = setTimeout(() => resolve(false), 15000);
      }),
    ]);
    if (!completed)
      log(
        '[FINDING] PAGE_LOAD_TIMEOUT load event not observed within 15000ms; page may still be rendering'
      );
  } finally {
    clearTimeout(timer);
    cdp.off('Page.loadEventFired', onLoad);
  }
}

/** Navigates the attached tab and waits up to 15s for load; returns the URL it reached. */
export async function navigateTab(cdp, url, log) {
  await cdp.send('Page.enable');
  await loadOrTimeout(cdp, url, log);
  const location = await cdp.send('Runtime.evaluate', {
    expression: 'location.href',
    returnByValue: true,
  });
  return location.result?.value || url;
}
