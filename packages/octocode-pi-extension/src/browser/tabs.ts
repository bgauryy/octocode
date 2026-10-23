import { setTimeout as sleep } from 'node:timers/promises';
import { openTab, pageTargets, type PageTarget } from './devtools.js';
import { httpOrigin, type BrowserSession } from './session-state.js';
import { switchTo } from './session.js';

/** What the agent opening a private page records for the tab that loads it (see `BrowserTool.open`); nothing for a public page. */
export type Approval = (tab: string) => void;

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
  const lines = tabs.map((tab, index) => `${tab.id === session.targetId ? '*' : ' '} [${index + 1}] ${tab.title || '(untitled)'} — ${tab.url ?? ''}`);
  return `Tabs (${tabs.length}; * = driven):\n${lines.join('\n')}\ntab with index switches to one; tab with url opens a new one.`;
}

/** How long a navigation to a new page may take to load (`navigate`'s load event, a new tab's first load). */
export const NAVIGATE_LOAD_MS = 20_000;

/** Waits (up to `timeoutMs`) until the driven page has finished loading, for a tab that was already loading when found. */
async function whenLoaded(session: BrowserSession, signal?: AbortSignal, timeoutMs = 10_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const state = await session.page.evaluate<string>('document.readyState', signal).catch(() => 'loading');
    if (state === 'complete') return;
    await sleep(150, undefined, signal ? { signal } : undefined);
  }
}

/**
 * Switches to tab number `index` (from `describeTabs`). Driving another tab is the agent choosing its page, like
 * `navigate`: `open` runs first for a tab on a web address (the permission check, a private host included), and a
 * refusal leaves the session where it was.
 */
export async function selectTab(session: BrowserSession, index: number | undefined, open: (url: string) => Promise<Approval>, signal?: AbortSignal): Promise<void> {
  const tabs = await orderedTabs(session);
  const tab = index === undefined ? undefined : tabs[index - 1];
  if (!tab) throw new Error(`No tab ${index ?? ''}. ${await describeTabs(session)}`);
  if (tab.id !== session.targetId) {
    if (httpOrigin(tab.url)) (await open(tab.url!))(tab.id);
    await switchTo(session, tab);
  }
  await whenLoaded(session, signal);
}

/** Opens `url` in a new tab and drives it, `approve` recording what the agent's permission allowed for that tab. The tab opens blank, so the session's policy sees the first load too. */
export async function newTab(session: BrowserSession, url: string, approve: Approval, signal?: AbortSignal): Promise<void> {
  await switchTo(session, await openTab(session.port, 'about:blank'));
  approve(session.targetId);
  delete session.blocked;
  const result = await session.page.send<{ errorText?: string }>('Page.navigate', { url }, signal);
  if (result.errorText) throw new Error(`Navigation failed: ${result.errorText}${session.blocked ? ` (blocked ${session.blocked})` : ''}`);
  await whenLoaded(session, signal, NAVIGATE_LOAD_MS);
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
 * it, as a user's attention would; returns the tab, or undefined when none opened. The browser held the popup before it
 * ran until the session's network setup was on it (see `guardTargets` in `page-network.ts`), so its first request was checked too.
 */
export async function followNewTab(session: BrowserSession, before: Set<string> | undefined, signal?: AbortSignal): Promise<PageTarget | undefined> {
  if (!before) return undefined;
  const opened = (await pageTargets(session.port).catch(() => [])).find((target) => !before.has(target.id));
  if (!opened) return undefined;
  await switchTo(session, opened);
  await whenLoaded(session, signal);
  return opened;
}
