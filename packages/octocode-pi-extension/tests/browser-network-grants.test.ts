import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeCtx } from './fake-pi.js';
import { navigates, openFor, useNetworkHarness } from './browser-network-harness.js';
import { answered, paused } from './fixtures/cdp-stub.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { openBrowser } = await import('../src/browser/session.js');
const { setup, stub } = useNetworkHarness();

beforeEach(() => launch.mockReset());

describe('browser page requests', () => {
  it('scopes "Allow once" to the tab that opened the private host, and ends it when the tab navigates elsewhere', async () => {
    const { cdp, browser } = await setup({ mode: 'manual' });
    const ctx = fakeCtx({ cwd: '/private/tmp' });
    ctx.ui.select = async (_title, choices) => choices[0];
    browser.useContext(ctx, 'call-1');
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    await openFor(browser, 'http://127.0.0.1:3000/');
    navigates(page, 'http://127.0.0.1:3000/app');
    // A frame of the page is not the tab navigating.
    navigates(page, 'https://example.com/ad', { id: 'ad', parentId: 'main' });
    page.emit('Fetch.requestPaused', paused('own', 'http://127.0.0.1:3000/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toEqual(['own']));
    // The tab goes to a public page: that page's script cannot reach the host the agent opened earlier.
    navigates(page, 'https://example.com/');
    page.emit('Fetch.requestPaused', paused('hostile', 'http://127.0.0.1:3000/secret', 'Fetch'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['hostile']));
  });

  it('lets a popup of a once-approved tab load the host the tab was opened on, and a popup of any other tab nothing private', async () => {
    const { cdp, browser } = await setup({ mode: 'full' });
    const session = await browser.session();
    await openFor(browser, 'http://127.0.0.1:3000/');
    const popup = cdp.autoAttach({ id: 'popup-1', type: 'page', openerId: session.targetId });
    await vi.waitFor(() => expect(cdp.commands.some((command) => command.method === 'Fetch.enable')).toBe(true));
    popup.emit('Fetch.requestPaused', paused('own', 'http://127.0.0.1:3000/popup.js', 'Script'));
    popup.emit('Fetch.requestPaused', paused('other', 'http://127.0.0.1:3001/popup.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toContain('own'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toContain('other'));
  });

  it('keeps a session grant for the whole browser session, across navigations of the tab', async () => {
    const { cdp, browser } = await setup({ mode: 'manual' });
    const ctx = fakeCtx({ cwd: '/private/tmp' });
    const select = vi.fn(async (_title: string, choices: string[]) => choices.find((choice) => choice.endsWith('for this session') && choice.includes('127.0.0.1:3000')) ?? choices[0]);
    ctx.ui.select = select;
    browser.useContext(ctx, 'call-1');
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    await openFor(browser, 'http://127.0.0.1:3000/');
    navigates(page, 'https://example.com/');
    page.emit('Fetch.requestPaused', paused('later', 'http://127.0.0.1:3000/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toEqual(['later']));
    // The grant also answers the next open, with no new dialog.
    await openFor(browser, 'http://127.0.0.1:3000/again');
    expect(select).toHaveBeenCalledTimes(1);
    await browser.close();
    await browser.session();
    [...cdp.pages.values()].at(-1)!.emit('Fetch.requestPaused', paused('after', 'http://127.0.0.1:3000/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['after']));
  });
});

describe('browser page request grants, closed tabs and timeouts', () => {
  it('keys grants by host and effective port: a grant for http://localhost does not cover https://localhost', async () => {
    const { browser } = await setup({ mode: 'manual', lookup: async () => [{ address: '10.0.0.5' }] });
    const ctx = fakeCtx({ cwd: '/private/tmp' });
    const dialogs: string[][] = [];
    ctx.ui.select = async (_title: string, choices: string[]) => {
      dialogs.push(choices);
      return choices.find((choice) => choice.endsWith('for this session')) ?? choices[0];
    };
    browser.useContext(ctx, 'call-1');
    await browser.open('http://app.local/');
    await browser.open('http://app.local:80/other');
    expect(dialogs).toHaveLength(1);
    expect(dialogs[0]).toContain('Allow app.local:80 for this session');
    await browser.open('https://app.local/');
    expect(dialogs).toHaveLength(2);
    expect(dialogs[1]).toContain('Allow app.local:443 for this session');
    expect(browser.permission.grants()).toEqual(['net.private: app.local:80', 'net.private: app.local:443']);
  });

  it('drops a tab\'s approval when the tab closes', async () => {
    const { cdp, browser } = await setup({ mode: 'full' });
    const session = await browser.session();
    await openFor(browser, 'http://127.0.0.1:3000/');
    const page = cdp.pages.get(session.targetId)!;
    page.emit('Fetch.requestPaused', paused('own', 'http://127.0.0.1:3000/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toEqual(['own']));
    navigates(page, 'http://127.0.0.1:3000/app');
    await vi.waitFor(() => expect([...session.tabOrigins.values()]).toEqual(['http://127.0.0.1:3000']));
    expect(session.policy!.approves('127.0.0.1', 3000)).toBe(true);
    // Chrome reports the tab gone: its approval ends with it, and so does the origin it pinned.
    page.drop();
    await vi.waitFor(() => expect(session.tabOrigins.size).toBe(0));
    expect(session.policy!.approves('127.0.0.1', 3000)).toBe(false);
  });

  it('fails a paused request the policy does not answer within 5 seconds', async () => {
    const cdp = await stub();
    const report = vi.fn();
    const session = await openBrowser({}, { port: cdp.port, reuseTab: true, policy: { check: () => new Promise<never>(() => undefined), rebinds: () => false, report, navigated: () => undefined, opened: () => undefined, closed: () => undefined, approves: () => false } });
    const started = Date.now();
    [...cdp.pages.values()].at(-1)!.emit('Fetch.requestPaused', paused('slow', 'https://slow.example/x', 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['slow']), { timeout: 9_000 });
    expect(Date.now() - started).toBeGreaterThanOrEqual(4_900);
    expect(report).toHaveBeenCalledWith({ kind: 'blocked', url: 'https://slow.example/x', resourceType: 'XHR', reason: 'The request check timed out after 5s' });
    session.page.close();
  }, 20_000);

  it.each([
    ['the request fails', () => Promise.reject(new Error('connection refused'))],
    ['Chrome refuses', () => Promise.resolve(new Response('no', { status: 500 }))],
  ])('does not log the tab as closed when %s', async (_name, closeReply) => {
    const cdp = await stub();
    const policy = { check: async () => undefined, rebinds: () => true, report: vi.fn(), navigated: () => undefined, opened: () => undefined, closed: () => undefined, approves: () => false };
    const session = await openBrowser({}, { port: cdp.port, reuseTab: true, policy });
    const real = globalThis.fetch;
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => ((input instanceof Request ? input.url : String(input)).includes('/json/close/') ? closeReply() : real(input, init)));
    try {
      [...cdp.pages.values()].at(-1)!.emit('Network.responseReceived', { response: { url: 'https://cdn.example/c', remoteIPAddress: '10.0.0.5', remotePort: 443 } });
      await vi.waitFor(() => expect(session.console.some((line) => line.includes('answered from the private address 10.0.0.5'))).toBe(true));
      expect(session.console.at(-1)).toContain('the tab could not be closed');
      expect(session.console.join('\n')).not.toContain('the tab was closed');
      expect(policy.report).toHaveBeenCalledWith({ kind: 'rebinding', url: 'https://cdn.example/c', address: '10.0.0.5' });
    } finally {
      fetchSpy.mockRestore();
      session.page.close();
    }
  });
});

describe('browser DNS rebinding and WebSockets', () => {
  it('closes the tab and audits a public name that answers from a private address', async () => {
    const { cdp, browser, audit } = await setup();
    const session = await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    // Another port than the URL's is a proxy: not a rebinding. A public address is fine.
    page.emit('Network.responseReceived', { response: { url: 'https://cdn.example/a', remoteIPAddress: '127.0.0.1', remotePort: 3128 } });
    page.emit('Network.responseReceived', { response: { url: 'https://cdn.example/b', remoteIPAddress: '93.184.216.34', remotePort: 443 } });
    page.emit('Network.responseReceived', { response: { url: 'http://127.0.0.1:5000/', remoteIPAddress: '127.0.0.1', remotePort: 5000 } });
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(cdp.requests.some((request) => request.startsWith('GET /json/close/'))).toBe(false);

    page.emit('Network.responseReceived', { response: { url: 'https://cdn.example/c', remoteIPAddress: '10.0.0.5', remotePort: 443 } });
    await vi.waitFor(() => expect(cdp.requests).toContain(`GET /json/close/${session.targetId}`));
    await vi.waitFor(() => expect(session.console.some((line) => line.includes('answered from the private address 10.0.0.5; the tab was closed'))).toBe(true));
    expect(audit().at(-1)).toMatchObject({ action: 'invariant', allowed: false });
    expect(audit().at(-1)!.note).toMatch(/closing the tab: a public name answered from a private address \(DNS rebinding\)/);
  });

  it('reports a WebSocket to a private host, as audit entry and warning, without blocking it', async () => {
    const { cdp, browser, audit, text } = await setup();
    const session = await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    page.emit('Network.webSocketCreated', { requestId: 'w1', url: 'wss://93.184.216.34/live' });
    page.emit('Network.webSocketCreated', { requestId: 'w2', url: 'ws://127.0.0.1:4000/socket' });
    await vi.waitFor(() => expect(session.warning).toMatch(/^The page opened a WebSocket it may not \(ws:\/\/127\.0\.0\.1:4000\/socket\): Blocked private or local address: 127\.0\.0\.1:4000\./));
    expect(session.console.some((line) => line.startsWith('[warning] The page opened a WebSocket'))).toBe(true);
    const reported = audit().filter((entry) => entry.via === 'reported');
    expect(reported).toHaveLength(1);
    expect(reported[0]).toMatchObject({ action: 'invariant', allowed: true, what: ['ws://127.0.0.1:4000/socket'] });
    expect(reported[0].note).toMatch(/^browser: a page WebSocket was not blocked, reported only/);
    // Nothing was failed or closed: Fetch cannot stop a handshake, and the report says so.
    expect(answered(cdp, 'Fetch.failRequest')).toEqual([]);
    // The next result that carries page notes leads with it, once.
    expect(await text({ action: 'tabs' })).not.toContain('Warning:');
    expect(await text({ action: 'tab', url: 'https://93.184.216.34/' })).toMatch(/^Warning: The page opened a WebSocket it may not/);
    expect(await text({ action: 'tab', url: 'https://93.184.216.34/' })).not.toContain('Warning:');
  }, 20_000);

  it('does not report a WebSocket to the host the agent opened', async () => {
    const { cdp, browser, audit } = await setup();
    const session = await browser.session();
    await openFor(browser, 'http://127.0.0.1:4000/');
    [...cdp.pages.values()].at(-1)!.emit('Network.webSocketCreated', { requestId: 'w', url: 'ws://127.0.0.1:4000/hmr' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(session.warning).toBeUndefined();
    expect(audit().filter((entry) => entry.via === 'reported')).toEqual([]);
  });
});

describe('browser private pages the agent switches to', () => {
  it('applies net.private to a tab the agent switches to, before connecting to it', async () => {
    const targets = [
      { id: 'tab-0', type: 'page', url: 'https://93.184.216.34/' },
      { id: 'internal', type: 'page', url: 'http://127.0.0.1:3000/admin' },
      { id: 'blank', type: 'page', url: 'about:blank' },
    ];
    // Manual mode with no one to ask: refused, and the stub never sees a connection to that tab.
    const refused = await setup({ mode: 'manual', targets });
    await refused.browser.session();
    const order = await refused.text({ action: 'tabs' });
    const index = Number(/\[(\d+)\][^\n]*127\.0\.0\.1:3000/.exec(order)![1]);
    await expect(refused.run({ action: 'tab', index })).rejects.toThrow(/Blocked private or local address: 127\.0\.0\.1:3000\. Not run/);
    expect(refused.cdp.pages.has('internal')).toBe(false);
    // A tab with no web address needs no check.
    const blank = Number(/\[(\d+)\][^\n]*about:blank/.exec(order)![1]);
    await expect(refused.run({ action: 'tab', index: blank })).resolves.toBeDefined();
    expect(refused.cdp.pages.has('blank')).toBe(true);

    // Full mode allows it; the tab then loads from its own host, and still nothing else private.
    const allowed = await setup({ mode: 'full', targets });
    await allowed.browser.session();
    const listed = await allowed.text({ action: 'tabs' });
    await allowed.run({ action: 'tab', index: Number(/\[(\d+)\][^\n]*127\.0\.0\.1:3000/.exec(listed)![1]) });
    expect(allowed.cdp.pages.has('internal')).toBe(true);
    const page = allowed.cdp.pages.get('internal')!;
    page.emit('Fetch.requestPaused', paused('own', 'http://127.0.0.1:3000/x.js', 'Script'));
    page.emit('Fetch.requestPaused', paused('other', 'http://127.0.0.1:3001/x.js', 'Script'));
    await vi.waitFor(() => expect(answered(allowed.cdp, 'Fetch.continueRequest')).toContain('own'));
    await vi.waitFor(() => expect(answered(allowed.cdp, 'Fetch.failRequest')).toEqual(['other']));
  }, 20_000);

  it('never opens a dialog for a private page request, even in manual mode', async () => {
    const { cdp, browser } = await setup({ mode: 'manual' });
    const ctx = fakeCtx({ cwd: '/private/tmp' });
    const select = vi.fn();
    ctx.ui.select = select;
    browser.useContext(ctx, 'call-1');
    await browser.session();
    [...cdp.pages.values()].at(-1)!.emit('Fetch.requestPaused', paused('r', 'http://10.1.1.1/', 'Document'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['r']));
    expect(select).not.toHaveBeenCalled();
  });
});
