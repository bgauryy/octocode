import { afterEach, describe, expect, it, vi } from 'vitest';
import { CdpConnection, type AttachedTarget } from '../src/browser/cdp.js';
import type { BrowserSession } from '../src/browser/session-state.js';
import type { NetworkEvent } from '../src/browser/policy.js';
import { openBrowser, switchTo } from '../src/browser/session.js';
import { PagePolicy } from '../src/browser/network.js';
import { answered, methodsOn, paused, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';

const stubs: CdpStub[] = [];
const sessions: BrowserSession[] = [];
const connections: CdpConnection[] = [];

afterEach(async () => {
  for (const session of sessions.splice(0)) session.connection.close();
  for (const connection of connections.splice(0)) connection.close();
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
});


async function drive(handle?: CdpHandler) {
  // The driven page is on example.test: that origin's service workers belong to it (this is a Chrome the session attached to).
  const cdp = await startCdpStub({ ...(handle ? { handle } : {}), targets: [{ id: 'page-1', type: 'page', url: 'https://example.test/' }] });
  stubs.push(cdp);
  const events: NetworkEvent[] = [];
  const policy = new PagePolicy({ report: (event) => events.push(event), lookup: async () => [{ address: '93.184.216.34' }] });
  const session = await openBrowser({}, { port: cdp.port, reuseTab: true, policy });
  sessions.push(session);
  return { cdp, session, events };
}


describe('auto-attach', () => {
  it('asks the browser and the driven page for related targets, held for the debugger, once the page is covered', async () => {
    const { cdp } = await drive();
    const auto = { autoAttach: true, waitForDebuggerOnStart: true, flatten: true };
    expect(cdp.commands.filter((command) => command.method === 'Target.setAutoAttach')).toEqual([
      { target: 'page-1', method: 'Target.setAutoAttach', params: auto },
      { target: 'browser', method: 'Target.setAutoAttach', params: auto },
    ]);
    const page = methodsOn(cdp, 'page-1');
    expect(page.indexOf('Fetch.enable')).toBeLessThan(page.indexOf('Target.setAutoAttach'));
  });

  it('asks for nothing without a network policy', async () => {
    const cdp = await startCdpStub();
    stubs.push(cdp);
    sessions.push(await openBrowser({}, { port: cdp.port, reuseTab: true }));
    expect(cdp.commands.some((command) => command.method === 'Target.setAutoAttach')).toBe(false);
  });

  it('sets up a popup before it runs, fails its request to a private host and lets a public one through', async () => {
    const { cdp, session, events } = await drive();
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: 'page-1', url: '' });
    expect(popup.running).toBe(false);
    await vi.waitFor(() => expect(methodsOn(cdp, 'popup-1')).toContain('Runtime.runIfWaitingForDebugger'));
    const methods = methodsOn(cdp, 'popup-1');
    // Nothing the popup does can come before its setup: the release is the last thing, and the target was held until then.
    expect(methods.at(-1)).toBe('Runtime.runIfWaitingForDebugger');
    expect(methods.indexOf('Network.setBypassServiceWorker')).toBeLessThan(methods.indexOf('Fetch.enable'));
    expect(methods.indexOf('Fetch.enable')).toBeLessThan(methods.indexOf('Target.setAutoAttach'));
    expect(methods.indexOf('Target.setAutoAttach')).toBeLessThan(methods.indexOf('Runtime.runIfWaitingForDebugger'));
    expect(popup.running).toBe(true);

    popup.emit('Fetch.requestPaused', paused('private', 'http://10.0.0.1/', 'Document'));
    popup.emit('Fetch.requestPaused', paused('public', 'https://93.184.216.34/'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'popup-1')).toEqual(['private']));
    expect(answered(cdp, 'Fetch.continueRequest', 'popup-1')).toEqual(['public']);
    expect(events).toMatchObject([{ kind: 'blocked', url: 'http://10.0.0.1/', resourceType: 'Document' }]);
    expect(session.blocked).toContain('http://10.0.0.1/');
  });

  it('drives a popup through the session it was set up on, and covers a popup of that popup', async () => {
    const { cdp, session } = await drive();
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: 'page-1' });
    await vi.waitFor(() => expect(popup.running).toBe(true));
    const governed = session.governed.get('popup-1')!;
    expect(governed.open).toBe(true);
    await switchTo(session, { id: 'popup-1', webSocketDebuggerUrl: `ws://127.0.0.1:${cdp.port}/devtools/page/popup-1` });
    expect(session.page).toBe(governed);
    expect(session.targetId).toBe('popup-1');
    // The driven popup opens one more: the opener is a governed tab, so it is covered the same way.
    const second = cdp.autoAttach({ id: 'popup-2', type: 'page', openerId: 'popup-1' });
    await vi.waitFor(() => expect(second.running).toBe(true));
    expect(methodsOn(cdp, 'popup-2')).toContain('Fetch.enable');
  });

  it('lets a tab nobody here opened run, and drops the session it never needed', async () => {
    const { cdp, session } = await drive();
    // A tab of the user's, or one this session opens and attaches to itself: neither was opened by a governed page.
    const stranger = cdp.autoAttach({ id: 'user-tab', type: 'page' });
    await vi.waitFor(() => expect(stranger.running).toBe(true));
    expect(methodsOn(cdp, 'user-tab')).toEqual(['Runtime.runIfWaitingForDebugger']);
    expect(session.governed.has('user-tab')).toBe(false);
    // A tab that was already open (not held) is not released twice.
    const existing = cdp.autoAttach({ id: 'old-tab', type: 'page', waiting: false });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(methodsOn(cdp, 'old-tab')).toEqual([]);
    expect(existing.running).toBe(true);
    // The driven page's own session keeps working.
    await expect(session.page.send('Runtime.evaluate', { expression: '1' })).resolves.toBeDefined();
  });

  it('sets a dedicated worker up before it runs, keeps the page pausing its fetches, and its detaching leaves the page alone', async () => {
    const { cdp, session } = await drive();
    const worker = cdp.autoAttach({ id: 'worker-1', type: 'worker', parent: 'page-1', url: 'http://example.test/worker.js' });
    await vi.waitFor(() => expect(worker.running).toBe(true));
    const methods = methodsOn(cdp, 'worker-1');
    // A dedicated worker has no Fetch domain (the page that owns it pauses its requests) and no service worker or cache to switch off.
    expect(methods).toEqual(['Network.enable', 'Target.setAutoAttach', 'Runtime.runIfWaitingForDebugger']);
    // Its fetches are paused on the page's session, which keeps answering.
    cdp.pages.get('page-1')!.emit('Fetch.requestPaused', paused('w-private', 'http://127.0.0.1:9/secret', 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'page-1')).toEqual(['w-private']));

    worker.drop();
    await expect(session.page.send('Runtime.evaluate', { expression: '1' })).resolves.toBeDefined();
    expect(session.page.open).toBe(true);
    expect(session.connection.open).toBe(true);
  });

  it('pauses a service worker\'s own fetches, which no page session sees', async () => {
    const { cdp, events } = await drive();
    const worker = cdp.autoAttach({ id: 'sw-1', type: 'service_worker', url: 'https://example.test/sw.js' });
    await vi.waitFor(() => expect(worker.running).toBe(true));
    expect(methodsOn(cdp, 'sw-1')).toEqual(['Network.enable', 'Fetch.enable', 'Target.setAutoAttach', 'Runtime.runIfWaitingForDebugger']);
    worker.emit('Fetch.requestPaused', paused('sw-private', 'http://192.168.1.1/'));
    worker.emit('Fetch.requestPaused', paused('sw-public', 'https://93.184.216.34/api'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest', 'sw-1')).toEqual(['sw-private']));
    expect(answered(cdp, 'Fetch.continueRequest', 'sw-1')).toEqual(['sw-public']);
    expect(events).toMatchObject([{ kind: 'blocked', url: 'http://192.168.1.1/', resourceType: 'XHR' }]);
  });

  it('reports a private address a worker\'s response came from, without a tab to close', async () => {
    const { cdp, session, events } = await drive();
    const worker = cdp.autoAttach({ id: 'worker-1', type: 'worker', parent: 'page-1' });
    await vi.waitFor(() => expect(worker.running).toBe(true));
    worker.emit('Network.responseReceived', { response: { url: 'https://rebind.example/x', remoteIPAddress: '10.0.0.5', remotePort: 443 } });
    await vi.waitFor(() => expect(events).toMatchObject([{ kind: 'rebinding', url: 'https://rebind.example/x', address: '10.0.0.5' }]));
    expect(cdp.requests.some((request) => request.startsWith('GET /json/close/'))).toBe(false);
    expect(session.console.at(-1)).toContain('answered from the private address 10.0.0.5');
  });

  it('leaves what is not a web target alone', async () => {
    const { cdp } = await drive();
    const extension = cdp.autoAttach({ id: 'ext-sw', type: 'service_worker', url: 'chrome-extension://abc/sw.js' });
    const other = cdp.autoAttach({ id: 'bg', type: 'background_page', url: 'chrome-extension://abc/bg.html' });
    await vi.waitFor(() => expect(extension.running && other.running).toBe(true));
    expect(methodsOn(cdp, 'ext-sw')).toEqual(['Runtime.runIfWaitingForDebugger']);
    expect(methodsOn(cdp, 'bg')).toEqual(['Runtime.runIfWaitingForDebugger']);
  });

  it('closes a target whose setup fails', async () => {
    const { cdp, session, events } = await drive((method, _params, page) => (page.id === 'popup-1' && method === 'Fetch.enable' ? { error: { message: 'Fetch is off' } } : undefined));
    cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: 'page-1', url: 'http://10.0.0.1/' });
    await vi.waitFor(() => expect(cdp.commands.some((command) => command.method === 'Target.closeTarget')).toBe(true));
    expect(cdp.commands.find((command) => command.method === 'Target.closeTarget')).toMatchObject({ target: 'browser', params: { targetId: 'popup-1' } });
    expect(session.governed.has('popup-1')).toBe(false);
    expect(events).toMatchObject([{ kind: 'blocked', resourceType: 'page' }]);
  });
});

describe('the connection and auto-attached sessions', () => {
  const connect = async () => {
    const stub = await startCdpStub({ targets: [{ id: 'a', type: 'page' }] });
    stubs.push(stub);
    const connection = await CdpConnection.connect(stub.port);
    connections.push(connection);
    const attached: AttachedTarget[] = [];
    connection.onAttached((target) => attached.push(target));
    return { stub, connection, attached };
  };

  it('does not take the attach it asked for for an auto-attach, but does take one held for the debugger', async () => {
    const { stub, connection, attached } = await connect();
    await connection.attach({ id: 'a', webSocketDebuggerUrl: `ws://127.0.0.1:${stub.port}/devtools/page/a` });
    expect(attached).toEqual([]);
    stub.autoAttach({ id: 'w', type: 'worker', waiting: true });
    stub.autoAttach({ id: 'x', type: 'worker', waiting: false });
    await vi.waitFor(() => expect(attached.map((target) => target.info.targetId)).toEqual(['w', 'x']));
    expect(attached.map((target) => target.waiting)).toEqual([true, false]);
  });

  it('routes commands and events on an auto-attached session, and ends only the session that detached', async () => {
    const { stub, connection, attached } = await connect();
    const page = await connection.attach({ id: 'a', webSocketDebuggerUrl: `ws://127.0.0.1:${stub.port}/devtools/page/a` });
    const worker = stub.autoAttach({ id: 'w', type: 'worker', parent: 'a' });
    await vi.waitFor(() => expect(attached).toHaveLength(1));
    const session = attached[0]!.page;
    const seen: string[] = [];
    session.on((method) => seen.push(method));
    worker.emit('Runtime.executionContextCreated');
    await session.send('Ping');
    expect(seen).toEqual(['Runtime.executionContextCreated']);
    expect(stub.commands.at(-1)).toMatchObject({ target: 'w', method: 'Ping' });

    worker.drop();
    await vi.waitFor(() => expect(session.open).toBe(false));
    expect(page.open).toBe(true);
    await expect(page.send('Ping')).resolves.toBeDefined();
    await expect(session.send('Ping')).rejects.toThrow('Browser connection closed');
  });
});
