import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { answered, methodsOn, paused, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';
import { PagePolicy } from '../src/browser/network.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { openBrowser, switchTo } = await import('../src/browser/session.js');
type Session = Awaited<ReturnType<typeof openBrowser>>;

const stubs: CdpStub[] = [];
const sessions: Session[] = [];
const dirs: string[] = [];

beforeEach(() => launch.mockReset());
afterEach(async () => {
  vi.unstubAllEnvs();
  for (const session of sessions.splice(0)) {
    session.connection.close();
    await session.chrome?.kill();
  }
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
  for (const dir of dirs.splice(0)) fs.rmSync(dir, { recursive: true, force: true });
});

const socketOf = (cdp: CdpStub, id: string) => ({ id, webSocketDebuggerUrl: `ws://127.0.0.1:${cdp.port}/devtools/page/${id}` });
const policy = () => new PagePolicy({ report: () => undefined, lookup: async () => [{ address: '93.184.216.34' }] });
const tmp = (): string => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-tab-policy-'));
  dirs.push(dir);
  return dir;
};

async function stub(handle?: CdpHandler, urls: Record<string, string> = {}) {
  const cdp = await startCdpStub({
    ...(handle ? { handle } : {}),
    targets: [
      { id: 'page-1', type: 'page', url: urls['page-1'] ?? 'https://example.test/' },
      { id: 'page-2', type: 'page', ...(urls['page-2'] ? { url: urls['page-2'] } : {}) },
    ],
  });
  stubs.push(cdp);
  return cdp;
}

/** The user's own Chrome: driven in place (`attach`), so only what is related to the tabs the session drives is covered. */
async function attached(handle?: CdpHandler, urls?: Record<string, string>) {
  const cdp = await stub(handle, urls);
  const session = await openBrowser({}, { port: cdp.port, reuseTab: true, policy: policy() });
  sessions.push(session);
  return { cdp, session };
}

/** The agent's own Chrome: everything the browser attaches is covered. */
async function launched() {
  const cdp = await stub();
  launch.mockResolvedValue({ port: cdp.port, kill: vi.fn(async () => undefined), process: { unref: vi.fn() } });
  vi.stubEnv('OCTOCODE_HOME', tmp());
  vi.stubEnv('OCTOCODE_CHROME_PORT', '');
  const session = await openBrowser({}, { policy: policy() });
  sessions.push(session);
  expect(session.kind).toBe('own');
  return { cdp, session };
}

describe('a tab the session moved away from', () => {
  it('stays attached and governed: its requests, popups and workers are still checked', async () => {
    const { cdp, session } = await attached();
    const first = cdp.pages.get('page-1')!;
    await switchTo(session, socketOf(cdp, 'page-2'));
    expect(session.targetId).toBe('page-2');
    expect(first.attached).toBe(true);
    expect(session.governed.get('page-1')?.open).toBe(true);

    // fetch() from the previous tab to a private host is failed, a public one continues.
    first.emit('Fetch.requestPaused', paused('private', 'http://10.0.0.1/'));
    first.emit('Fetch.requestPaused', paused('public', 'https://93.184.216.34/'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'page-1')).toEqual(['private']));
    expect(answered(cdp, 'Fetch.continueRequest', 'page-1')).toEqual(['public']);

    // window.open from the previous tab: the popup is held, set up, and its private request failed.
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: 'page-1' });
    await vi.waitFor(() => expect(popup.running).toBe(true));
    expect(methodsOn(cdp, 'popup-1')).toContain('Fetch.enable');
    popup.emit('Fetch.requestPaused', paused('popup-private', 'http://127.0.0.1:9/', 'Document'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'popup-1')).toEqual(['popup-private']));

    // A worker of the previous tab, and one it nests.
    const worker = cdp.autoAttach({ id: 'worker-1', type: 'worker', parent: 'page-1', url: 'https://example.test/w.js' });
    await vi.waitFor(() => expect(worker.running).toBe(true));
    expect(methodsOn(cdp, 'worker-1')).toContain('Network.enable');
    const nested = cdp.autoAttach({ id: 'worker-2', type: 'worker', parent: 'worker-1', url: 'https://example.test/n.js' });
    await vi.waitFor(() => expect(nested.running).toBe(true));
    expect(methodsOn(cdp, 'worker-2')).toContain('Network.enable');
  });

  it('is driven again without a second setup, and does not speak for the session while in the background', async () => {
    const { cdp, session } = await attached();
    const first = session.page;
    const firstPage = cdp.pages.get('page-1')!;
    firstPage.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'logged on the first tab' }] });
    await vi.waitFor(() => expect(session.console).toEqual(['[log] logged on the first tab']));
    session.console.push('[log] from the first tab');
    await switchTo(session, socketOf(cdp, 'page-2'));
    // The discarded line is still in the console log, and a marker shows where the new tab's lines begin.
    expect(fs.readFileSync(session.consoleLog!.file!, 'utf8')).toMatch(/^\[log\] logged on the first tab\n\[tab\] console of \S+\n$/);
    // The earlier tab's lines are discarded and counted, so the console result can say so.
    expect(session.console).toEqual([]);
    expect(session.consoleLost).toEqual({ dropped: 0, cleared: 2 });
    // The background tab's console and dialogs do not show up as the driven page's.
    firstPage.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'from the background' }] });
    firstPage.emit('Page.javascriptDialogOpening', { type: 'alert', message: 'hi' });
    cdp.pages.get('page-2')!.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'driven' }] });
    await vi.waitFor(() => expect(session.console).toEqual(['[log] driven']));
    expect(session.dialog).toBeUndefined();

    const fetchEnables = methodsOn(cdp, 'page-1').filter((method) => method === 'Fetch.enable').length;
    await switchTo(session, socketOf(cdp, 'page-1'));
    expect(session.page).toBe(first);
    expect(methodsOn(cdp, 'page-1').filter((method) => method === 'Fetch.enable')).toHaveLength(fetchEnables);
    firstPage.emit('Page.javascriptDialogOpening', { type: 'alert', message: 'now' });
    await vi.waitFor(() => expect(session.dialog).toEqual({ type: 'alert', message: 'now' }));
    // Each tab's events are handled once: switching back registered no second listener.
    firstPage.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'once' }] });
    await vi.waitFor(() => expect(session.console.filter((line) => line === '[log] once')).toHaveLength(1));
  });

  it('applies a download folder chosen in the meantime when it is driven again', async () => {
    const { cdp, session } = await attached();
    await switchTo(session, socketOf(cdp, 'page-2'));
    session.downloadDir = '/tmp/downloads';
    await switchTo(session, socketOf(cdp, 'page-1'));
    expect(cdp.commands.filter((command) => command.target === 'page-1' && command.method === 'Browser.setDownloadBehavior')).toHaveLength(1);
  });

  it('ends with its tab, and with the browser session', async () => {
    const { cdp, session } = await attached();
    await switchTo(session, socketOf(cdp, 'page-2'));
    const first = session.governed.get('page-1')!;
    await fetch(`http://127.0.0.1:${cdp.port}/json/close/page-1`);
    await vi.waitFor(() => expect(first.open).toBe(false));
    // A closed tab leaves the governed set the next time a target is attached or a tab is switched to.
    cdp.autoAttach({ id: 'user-tab', type: 'page' });
    await vi.waitFor(() => expect(session.governed.has('page-1')).toBe(false));
    expect(session.governed.get('page-2')?.open).toBe(true);

    session.connection.close();
    await vi.waitFor(() => expect(session.governed.get('page-2')?.open).toBe(false));
  });

  it('keeps every tab it drove attached, however many', async () => {
    const { cdp, session } = await attached();
    const ids = Array.from({ length: 6 }, (_, index) => `many-${index}`);
    for (const id of ids) cdp.addTarget(id);
    for (const id of ids) await switchTo(session, socketOf(cdp, id));
    expect([...session.governed.keys()].sort()).toEqual(['page-1', ...ids].sort());
    expect([...session.governed.values()].every((page) => page.open)).toBe(true);
  });
});

describe('the user\'s Chrome (attached or shared)', () => {
  it('releases an unrelated service worker and a tab of the user\'s at once, without a policy', async () => {
    const { cdp, session } = await attached();
    const sw = cdp.autoAttach({ id: 'sw-other', type: 'service_worker', url: 'https://other.test/sw.js' });
    const shared = cdp.autoAttach({ id: 'shared-other', type: 'shared_worker', url: 'https://other.test/s.js' });
    const tab = cdp.autoAttach({ id: 'user-tab', type: 'page', url: 'https://mail.test/' });
    const strangerPopup = cdp.autoAttach({ id: 'user-popup', type: 'page', openerId: 'user-tab' });
    const strangerWorker = cdp.autoAttach({ id: 'user-worker', type: 'worker', parent: 'user-tab', url: 'https://mail.test/w.js' });
    await vi.waitFor(() => expect([sw, shared, tab, strangerPopup, strangerWorker].every((target) => target.running && !target.attached)).toBe(true));
    for (const id of ['sw-other', 'shared-other', 'user-tab', 'user-popup', 'user-worker']) expect(methodsOn(cdp, id)).toEqual(['Runtime.runIfWaitingForDebugger']);
    expect([...session.governed.keys()]).toEqual(['page-1']);
    expect(session.workers.size).toBe(0);
    // The session's own tab is untouched by all of it.
    expect(cdp.pages.get('page-1')!.attached).toBe(true);
  });

  it('covers a popup of a governed tab, a worker of it, and a service or shared worker of its origin', async () => {
    const { cdp, session } = await attached();
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: 'page-1' });
    const worker = cdp.autoAttach({ id: 'worker-1', type: 'worker', parent: 'page-1', url: 'https://example.test/w.js' });
    const sw = cdp.autoAttach({ id: 'sw-1', type: 'service_worker', url: 'https://example.test/sw.js' });
    const shared = cdp.autoAttach({ id: 'shared-1', type: 'shared_worker', url: 'https://example.test/s.js' });
    await vi.waitFor(() => expect([popup, worker, sw, shared].every((target) => target.running)).toBe(true));
    expect(methodsOn(cdp, 'popup-1')).toContain('Fetch.enable');
    expect(methodsOn(cdp, 'worker-1')).toEqual(['Network.enable', 'Target.setAutoAttach', 'Runtime.runIfWaitingForDebugger']);
    expect(methodsOn(cdp, 'sw-1')).toEqual(['Network.enable', 'Fetch.enable', 'Target.setAutoAttach', 'Runtime.runIfWaitingForDebugger']);
    expect(methodsOn(cdp, 'shared-1')).toContain('Fetch.enable');
    sw.emit('Fetch.requestPaused', paused('sw-private', 'http://192.168.1.1/'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'sw-1')).toEqual(['sw-private']));
    expect([...session.workers.keys()].sort()).toEqual(['shared-1', 'sw-1', 'worker-1']);
  });

  it('covers the service worker of an origin a governed tab navigated to, and of a tab it switched to', async () => {
    const { cdp, session } = await attached(undefined, { 'page-2': 'https://second.test/' });
    const before = cdp.autoAttach({ id: 'sw-later', type: 'service_worker', url: 'https://later.test/sw.js' });
    await vi.waitFor(() => expect(before.running).toBe(true));
    expect(methodsOn(cdp, 'sw-later')).toEqual(['Runtime.runIfWaitingForDebugger']);

    cdp.pages.get('page-1')!.emit('Fetch.requestPaused', paused('nav', 'https://later.test/', 'Document'));
    await switchTo(session, { ...socketOf(cdp, 'page-2'), url: 'https://second.test/' });
    const after = cdp.autoAttach({ id: 'sw-later-2', type: 'service_worker', url: 'https://later.test/sw.js' });
    const second = cdp.autoAttach({ id: 'sw-second', type: 'service_worker', url: 'https://second.test/sw.js' });
    await vi.waitFor(() => expect(after.running && second.running).toBe(true));
    expect(methodsOn(cdp, 'sw-later-2')).toContain('Fetch.enable');
    expect(methodsOn(cdp, 'sw-second')).toContain('Fetch.enable');
  });

  it('detaches an unrelated target that was already running, without a word to it', async () => {
    const { cdp } = await attached();
    const running = cdp.autoAttach({ id: 'sw-running', type: 'service_worker', url: 'https://other.test/sw.js', waiting: false });
    await vi.waitFor(() => expect(running.attached).toBe(false));
    expect(methodsOn(cdp, 'sw-running')).toEqual([]);
  });

  it('does the same in a shared Chrome with a tab of the agent\'s own, and covers its popups', async () => {
    const cdp = await stub();
    const session = await openBrowser({}, { port: cdp.port, policy: policy() });
    sessions.push(session);
    expect(session.kind).toBe('shared');
    const stranger = cdp.autoAttach({ id: 'sw-other', type: 'service_worker', url: 'https://other.test/sw.js' });
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: session.targetId });
    await vi.waitFor(() => expect(stranger.running && popup.running).toBe(true));
    expect(methodsOn(cdp, 'sw-other')).toEqual(['Runtime.runIfWaitingForDebugger']);
    expect(methodsOn(cdp, 'popup-1')).toContain('Fetch.enable');
  });
});

describe('the agent\'s own Chrome', () => {
  it('covers every web target, and still lets the tabs and extensions it did not open run', async () => {
    const { cdp, session } = await launched();
    const sw = cdp.autoAttach({ id: 'sw-any', type: 'service_worker', url: 'https://anywhere.test/sw.js' });
    const shared = cdp.autoAttach({ id: 'shared-any', type: 'shared_worker', url: 'https://anywhere.test/s.js' });
    const worker = cdp.autoAttach({ id: 'worker-any', type: 'worker', url: 'https://anywhere.test/w.js' });
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: session.targetId });
    const stranger = cdp.autoAttach({ id: 'other-tab', type: 'page' });
    const extension = cdp.autoAttach({ id: 'ext-sw', type: 'service_worker', url: 'chrome-extension://abc/sw.js' });
    await vi.waitFor(() => expect([sw, shared, worker, popup, stranger, extension].every((target) => target.running)).toBe(true));
    for (const id of ['sw-any', 'shared-any', 'popup-1']) expect(methodsOn(cdp, id)).toContain('Fetch.enable');
    expect(methodsOn(cdp, 'worker-any')).toContain('Network.enable');
    expect(methodsOn(cdp, 'other-tab')).toEqual(['Runtime.runIfWaitingForDebugger']);
    expect(methodsOn(cdp, 'ext-sw')).toEqual(['Runtime.runIfWaitingForDebugger']);
  });

  it('covers the persistent profile\'s Chrome the same way, started by an earlier session', async () => {
    const home = tmp();
    const cdp = await stub();
    const profile = path.join(home, 'pi-browser-profile');
    fs.mkdirSync(profile, { recursive: true });
    fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${cdp.port}\n${cdp.browserPath}\n`);
    const session = await openBrowser({ OCTOCODE_HOME: home, OCTOCODE_BROWSER_VISIBLE: '1' }, { policy: policy() });
    sessions.push(session);
    expect(session.kind).toBe('shared');
    expect(session.profile).toBe(profile);
    const sw = cdp.autoAttach({ id: 'sw-any', type: 'service_worker', url: 'https://anywhere.test/sw.js' });
    await vi.waitFor(() => expect(sw.running).toBe(true));
    expect(methodsOn(cdp, 'sw-any')).toContain('Fetch.enable');
  });
});
