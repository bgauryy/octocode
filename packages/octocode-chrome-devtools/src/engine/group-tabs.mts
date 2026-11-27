// Private tab-group lifecycle on one browser port. A private group is one isolated browser context
// (Target.createBrowserContext: its own cookies and storage) shared by the group's tabs; a
// non-private group puts its tabs in the default context. The session meta records the target ids
// (`groups`) and the context of each private group (`contexts`).
import { randomUUID } from 'node:crypto';
import { connectCDP } from './cdp-connection.mjs';
import { errorMessage } from './error-message.mjs';
import { urlSchemeRefusal } from './cdp-policy.mjs';
import { isAboutOrDataUrl } from './mandatory-stealth.mjs';
import { chromeEndpoint } from './cdp-runner-targets.mjs';
import {
  loadGroupTab,
  withStdoutOnStderr,
  type GroupTab,
} from './open-tab.mjs';
import {
  originAndPath,
  originViolation,
  readSessionMeta,
  updateSessionMeta,
} from './session-meta.mjs';

type Note = (line: string) => void;
type Browser = Awaited<ReturnType<typeof connectCDP>>;
export type GroupListTab = {
  targetId: string;
  url?: string;
  title?: string;
  alive: boolean;
};
export type GroupListEntry = {
  name: string;
  private: boolean;
  tabCount: number;
  tabs: GroupListTab[];
};
type TabOptions = { background?: boolean; newWindow?: boolean };

/** `g-` plus a short random id: the name of a group opened without one. */
export const generatedGroupName = () => `g-${randomUUID().slice(0, 6)}`;

async function connectBrowser(port: number | string): Promise<Browser> {
  const version = await chromeEndpoint(String(port)).version();
  return connectCDP(version.webSocketDebuggerUrl, {
    targetInfo: { type: 'browser' },
  });
}

const knownGroups = (groups: Record<string, string[]>) =>
  Object.keys(groups).length
    ? Object.keys(groups).join(', ')
    : '(none; group open creates one)';

/** Refuses every url outside the session's allowlist, before anything is created. */
function checkOrigins(urls: readonly string[], port: number | string) {
  const allowOrigins = readSessionMeta(port).allowOrigins;
  for (const url of urls) {
    const scheme = urlSchemeRefusal(url, 'group url', null);
    if (scheme) throw new Error(scheme);
    const violation = originViolation(url, allowOrigins);
    if (violation) throw new Error(`${url}: ${violation}`);
  }
  return allowOrigins;
}

/** One tab per url, in order, in `contextId` (the default context when unset); each reports its own load status. */
async function createTabs(
  browser: Browser,
  port: number | string,
  urls: readonly string[],
  contextId: string | undefined,
  options: TabOptions,
  allowOrigins: readonly string[] | undefined,
  note: Note,
  made: GroupTab[]
) {
  const chrome = chromeEndpoint(String(port));
  await withStdoutOnStderr(async () => {
    for (const url of urls) {
      const created = await browser.send('Target.createTarget', {
        url: isAboutOrDataUrl(url) ? url : 'about:blank',
        ...(contextId ? { browserContextId: contextId } : {}),
        ...(options.newWindow ? { newWindow: true } : {}),
        ...(options.background ? { background: true } : {}),
      });
      const targetId: string = created.targetId;
      const tab: GroupTab = { url, targetId, status: 'error: not loaded' };
      made.push(tab);
      tab.status = await loadGroupTab(
        chrome,
        targetId,
        url,
        note,
        allowOrigins
      ).catch(error => `error: ${errorMessage(error)}`);
    }
  });
}

/** Creates the group: its context when private, one tab per url, then records it in the session meta. */
export async function openGroup(
  port: number,
  request: {
    name?: string;
    urls: string[];
    private: boolean;
    background: boolean;
    newWindow: boolean;
  },
  note: Note
) {
  const name = request.name ?? generatedGroupName();
  const meta = readSessionMeta(port);
  if (meta.groups && Object.hasOwn(meta.groups, name))
    throw new Error(
      `Group "${name}" already exists on port ${port}; pick another name or group close it first.`
    );
  const allowOrigins = checkOrigins(request.urls, port);
  const browser = await connectBrowser(port);
  const tabs: GroupTab[] = [];
  let contextId: string | undefined;
  try {
    if (request.private)
      contextId = (await browser.send('Target.createBrowserContext', {}))
        .browserContextId;
    await createTabs(
      browser,
      port,
      request.urls,
      contextId,
      request,
      allowOrigins,
      note,
      tabs
    );
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
  const latest = readSessionMeta(port);
  updateSessionMeta(port, {
    groups: { ...latest.groups, [name]: tabs.map(tab => tab.targetId) },
    contexts: contextId
      ? { ...latest.contexts, [name]: contextId }
      : latest.contexts,
  });
  return { group: name, private: Boolean(contextId), tabs };
}

/** Appends tabs to an existing group, in its context and under the same origin policy. */
export async function addToGroup(
  port: number,
  request: {
    name: string;
    urls: string[];
    background: boolean;
  },
  note: Note
) {
  const groups = readSessionMeta(port).groups ?? {};
  if (!Object.hasOwn(groups, request.name))
    throw new Error(
      `Unknown group "${request.name}" on port ${port}. Groups: ${knownGroups(groups)}`
    );
  const contextId = readSessionMeta(port).contexts?.[request.name];
  const allowOrigins = checkOrigins(request.urls, port);
  const browser = await connectBrowser(port);
  const tabs: GroupTab[] = [];
  try {
    await createTabs(
      browser,
      port,
      request.urls,
      contextId,
      { background: request.background },
      allowOrigins,
      note,
      tabs
    );
  } finally {
    browser.close();
    // Tabs already made belong to the group even when a later one failed.
    if (tabs.length) {
      const latest = readSessionMeta(port);
      if (latest.groups && Object.hasOwn(latest.groups, request.name))
        updateSessionMeta(port, {
          groups: {
            ...latest.groups,
            [request.name]: [
              ...latest.groups[request.name],
              ...tabs.map(tab => tab.targetId),
            ],
          },
        });
    }
  }
  return { group: request.name, private: Boolean(contextId), tabs };
}

/** Page targets Chrome currently lists, by id. */
async function livePages(browser: Browser) {
  const { targetInfos } = await browser.send('Target.getTargets', {});
  return new Map<string, { url: string; title: string }>(
    (
      targetInfos as {
        type: string;
        targetId: string;
        url: string;
        title: string;
      }[]
    )
      .filter(info => info.type === 'page')
      .map(info => [info.targetId, info])
  );
}

/** Drops from the meta the tabs Chrome no longer has; the group itself stays. */
function pruneDeadTabs(port: number, alive: ReadonlySet<string>) {
  const groups = readSessionMeta(port).groups;
  if (!groups) return;
  const pruned = Object.fromEntries(
    Object.entries(groups).map(([name, ids]) => [
      name,
      ids.filter(id => alive.has(id)),
    ])
  );
  updateSessionMeta(port, { groups: pruned });
}

/** Every group of this browser with its tabs reconciled against Chrome; closed tabs show alive:false once, then are pruned. */
export async function listGroups(port: number) {
  const meta = readSessionMeta(port);
  const browser = await connectBrowser(port);
  let live;
  try {
    live = await livePages(browser);
  } finally {
    browser.close();
  }
  const groups: GroupListEntry[] = Object.entries(meta.groups ?? {}).map(
    ([name, ids]) => ({
      name,
      private: Boolean(meta.contexts?.[name]),
      tabCount: ids.length,
      tabs: ids.map(targetId => {
        const page = live.get(targetId);
        return page
          ? {
              targetId,
              url: originAndPath(page.url),
              ...(page.title ? { title: page.title } : {}),
              alive: true,
            }
          : { targetId, alive: false };
      }),
    })
  );
  pruneDeadTabs(port, new Set(live.keys()));
  return { groups };
}

/** Closes one tab of the group, or every tab and then the group's context; returns what was closed. */
export async function closeGroup(
  port: number,
  request: { name: string; tab?: string }
) {
  const meta = readSessionMeta(port);
  const groups = meta.groups ?? {};
  const ids = Object.hasOwn(groups, request.name)
    ? groups[request.name]
    : undefined;
  if (!ids)
    throw new Error(
      `Unknown group "${request.name}" on port ${port}. Groups: ${knownGroups(groups)}`
    );
  if (request.tab !== undefined && !ids.includes(request.tab))
    throw new Error(
      `Tab ${request.tab} is not in group "${request.name}". Tabs: ${ids.join(', ') || '(none)'}`
    );
  const browser = await connectBrowser(port);
  try {
    if (request.tab !== undefined) {
      await browser
        .send('Target.closeTarget', { targetId: request.tab })
        .catch(() => {});
      const live = await livePages(browser);
      updateSessionMeta(port, {
        groups: {
          ...groups,
          [request.name]: ids.filter(id => id !== request.tab),
        },
      });
      pruneDeadTabs(port, new Set(live.keys()));
      return { group: request.name, closedTab: request.tab };
    }
    for (const targetId of ids)
      await browser.send('Target.closeTarget', { targetId }).catch(() => {});
    const contextId = meta.contexts?.[request.name];
    if (contextId)
      await browser
        .send('Target.disposeBrowserContext', { browserContextId: contextId })
        .catch(() => {});
  } finally {
    browser.close();
  }
  const { [request.name]: _gone, ...remaining } = groups;
  const { [request.name]: _context, ...contexts } = meta.contexts ?? {};
  updateSessionMeta(port, {
    groups: Object.keys(remaining).length ? remaining : undefined,
    contexts: Object.keys(contexts).length ? contexts : undefined,
  });
  return { group: request.name, closed: true, closedTabs: ids.length };
}

/** Disposes every private group's context on this port (cleanup); returns how many Chrome accepted. */
export async function disposeAllGroupContexts(
  port: number | string,
  workspace = process.cwd()
): Promise<number> {
  const contexts = Object.values(
    readSessionMeta(port, workspace).contexts ?? {}
  );
  if (!contexts.length) return 0;
  const browser = await connectBrowser(port);
  let disposed = 0;
  try {
    for (const browserContextId of contexts)
      await browser
        .send('Target.disposeBrowserContext', { browserContextId })
        .then(() => disposed++)
        .catch(() => {});
  } finally {
    browser.close();
  }
  return disposed;
}
