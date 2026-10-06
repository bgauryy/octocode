import { openTab, pageTargets, switchTo, type BrowserSession, type PageTarget } from './cdp.js';

/**
 * Tab numbers stay put for the session: tabs are numbered in the order this session first saw them, not in Chrome's
 * most-recently-used order, which changes every time a tab is switched to.
 */
const seenOrder = new WeakMap<BrowserSession, string[]>();

async function orderedTabs(session: BrowserSession): Promise<PageTarget[]> {
  const targets = await pageTargets(session.port);
  const order = seenOrder.get(session) ?? [];
  for (const target of [...targets].reverse()) if (!order.includes(target.id)) order.push(target.id);
  seenOrder.set(session, order);
  return [...targets].sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
}

/** One line per tab, the driven one marked `*`. */
export async function describeTabs(session: BrowserSession): Promise<string> {
  const tabs = await orderedTabs(session);
  const lines = tabs.map((tab, index) => `${tab.id === session.targetId ? '*' : ' '} [${index + 1}] ${(tab.title || '(untitled)').slice(0, 80)} — ${(tab.url ?? '').slice(0, 120)}`);
  return `Tabs (${tabs.length}; * = driven):\n${lines.join('\n')}\ntab with index switches to one; tab with url opens a new one.`;
}

/** Waits (up to `timeoutMs`) until the driven page has finished loading, for a tab that was already loading when found. */
async function whenLoaded(session: BrowserSession, timeoutMs = 10_000, signal?: AbortSignal): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const state = await session.page.evaluate<string>('document.readyState', signal).catch(() => 'loading');
    if (state === 'complete') return;
    await new Promise((resolve) => setTimeout(resolve, 150));
  }
}

/** Switches to tab number `index` (from `describeTabs`). */
export async function selectTab(session: BrowserSession, index: number | undefined, signal?: AbortSignal): Promise<void> {
  const tabs = await orderedTabs(session);
  const tab = index === undefined ? undefined : tabs[index - 1];
  if (!tab) throw new Error(`No tab ${index ?? ''}. ${await describeTabs(session)}`);
  if (tab.id !== session.targetId) await switchTo(session, tab);
  await whenLoaded(session, 10_000, signal);
}

/** Opens `url` in a new tab and drives it. */
export async function newTab(session: BrowserSession, url: string, signal?: AbortSignal): Promise<void> {
  await switchTo(session, await openTab(session.port, url));
  await whenLoaded(session, 20_000, signal);
}

/** Ids of the tabs open now, to spot a tab an action opens; undefined when the list cannot be read. */
export async function tabIds(session: BrowserSession): Promise<Set<string> | undefined> {
  return pageTargets(session.port).then(
    (targets) => new Set(targets.map((target) => target.id)),
    () => undefined,
  );
}

/**
 * When an action opened a tab (`window.open`, a popup, a link its script sent to a new window), the session moves to
 * it, as a user's attention would; returns the tab, or undefined when none opened.
 */
export async function followNewTab(session: BrowserSession, before: Set<string> | undefined, signal?: AbortSignal): Promise<PageTarget | undefined> {
  if (!before) return undefined;
  const opened = (await pageTargets(session.port).catch(() => [])).find((target) => !before.has(target.id));
  if (!opened) return undefined;
  await switchTo(session, opened);
  await whenLoaded(session, 10_000, signal);
  return opened;
}
