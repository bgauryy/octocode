import http from 'node:http';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { addressOf } from './doubles.js';
import { answered, navigates, openFor, paused, useNetworkHarness } from './browser-network-harness.js';
import type { PermissionMode } from '../src/permission/mode.js';
import type { HostLookup } from '../src/web/guard.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { stub, tool, fakeHome } = useNetworkHarness();

beforeEach(() => launch.mockReset());

const servers: http.Server[] = [];
const listen = (body: string): Promise<number> =>
  new Promise((resolve) => {
    const server = http.createServer((_request, response) => response.end(body));
    servers.push(server);
    server.listen(0, '127.0.0.1', () => resolve(addressOf(server).port));
  });
afterEach(() => {
  for (const server of servers.splice(0)) {
    server.closeAllConnections();
    server.close();
  }
});

/** The proxy's one `x-octocode-blocked` header; a repeated header fails the test. */
const blockedHeader = (value: string | string[] | undefined): string | undefined => {
  if (Array.isArray(value)) throw new Error(`expected one x-octocode-blocked header, got ${value.length}`);
  return value;
};

/** GET `url` through the proxy at `proxyUrl`, as Chrome sends it. */
const viaProxy = (proxyUrl: string, url: string): Promise<{ status: number; body: string; blocked: string | undefined }> =>
  new Promise((resolve, reject) => {
    const proxy = new URL(proxyUrl);
    http
      .get({ host: proxy.hostname, port: Number(proxy.port), path: url, headers: { host: new URL(url).host } }, (response) => {
        let body = '';
        response.on('data', (chunk: Buffer) => (body += chunk.toString()));
        response.on('end', () => resolve({ status: response.statusCode ?? 0, body, blocked: blockedHeader(response.headers['x-octocode-blocked']) }));
      })
      .on('error', reject);
  });

async function launched(mode: PermissionMode, lookup?: HostLookup) {
  const cdp = await stub();
  launch.mockResolvedValue({ port: cdp.port, kill: vi.fn(async () => undefined), process: { unref: vi.fn() } });
  vi.stubEnv('OCTOCODE_HOME', fakeHome());
  vi.stubEnv('OCTOCODE_CHROME_PORT', '');
  const { browser, audit } = tool(mode, lookup ? { lookup } : {});
  return { cdp, browser, audit };
}

describe('browser launched Chrome behind the filtering proxy', () => {
  it('lets the proxy through to targets the agent opened only, and audits what it refuses', async () => {
    const approvedPort = await listen('approved');
    const otherPort = await listen('other');
    const { cdp, browser } = await launched('manual');
    const session = await browser.session();
    const proxy = session.chrome!.proxy!;
    expect(launch.mock.calls[0][0].chromeFlags).toContain(`--proxy-server=${proxy.url}`);

    // Refused (manual, no one to ask), and a public target: neither is allowed through the proxy.
    await expect(browser.open(`http://127.0.0.1:${approvedPort}/`)).rejects.toThrow(/no one can be asked here/);
    // A page request never allows anything.
    [...cdp.pages.values()].at(-1)!.emit('Fetch.requestPaused', paused('r', `http://127.0.0.1:${approvedPort}/x`, 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['r']));
    expect((await viaProxy(proxy.url, `http://127.0.0.1:${approvedPort}/`)).status).toBe(403);

    // The agent opens one target (full mode: allowed without a prompt).
    const full = await launched('full');
    const fullSession = await full.browser.session();
    await openFor(full.browser, 'https://93.184.216.34/');
    await openFor(full.browser, `http://127.0.0.1:${approvedPort}/`);
    const fullProxy = fullSession.chrome!.proxy!.url;
    expect(await viaProxy(fullProxy, `http://127.0.0.1:${approvedPort}/`)).toMatchObject({ status: 200, body: 'approved' });
    expect(await viaProxy(fullProxy, `http://127.0.0.1:${otherPort}/`)).toMatchObject({ status: 403, blocked: 'private' });

    // A Chrome that starts later is a new browser session: nothing the agent opened before is allowed until it opens it again.
    await full.browser.close();
    const again = await full.browser.session();
    expect(await viaProxy(again.chrome!.proxy!.url, `http://127.0.0.1:${approvedPort}/`)).toMatchObject({ status: 403, blocked: 'private' });
    await openFor(full.browser, `http://127.0.0.1:${approvedPort}/`);
    expect(await viaProxy(again.chrome!.proxy!.url, `http://127.0.0.1:${approvedPort}/`)).toMatchObject({ status: 200, body: 'approved' });

    // The refusal is in the audit, once per host and browser session.
    expect(await viaProxy(again.chrome!.proxy!.url, `http://127.0.0.1:${otherPort}/`)).toMatchObject({ status: 403 });
    const proxied = full.audit().filter((entry) => entry.note?.includes('filtering proxy'));
    expect(proxied.map((entry) => entry.what)).toEqual([[`127.0.0.1:${otherPort}`], [`127.0.0.1:${approvedPort}`], [`127.0.0.1:${otherPort}`]]);
    expect(proxied[0]).toMatchObject({ action: 'invariant', allowed: false });
    expect(proxied[0].note).toBe('browser: the filtering proxy refused a page connection to a local or private address');
  }, 30_000);

  it('scopes the filtering proxy like the page policy: a once-approved host is open while its tab holds it', async () => {
    const approvedPort = await listen('approved');
    const { cdp, browser } = await launched('full');
    const session = await browser.session();
    await openFor(browser, `http://127.0.0.1:${approvedPort}/`);
    const proxyUrl = session.chrome!.proxy!.url;
    expect(await viaProxy(proxyUrl, `http://127.0.0.1:${approvedPort}/`)).toMatchObject({ status: 200, body: 'approved' });
    // The tab left the host: nothing keeps it open.
    navigates(cdp.pages.get(session.targetId)!, 'https://93.184.216.34/');
    expect(await viaProxy(proxyUrl, `http://127.0.0.1:${approvedPort}/`)).toMatchObject({ status: 403, blocked: 'private' });
  }, 30_000);
});

describe('browser launched Chrome behind the filtering proxy: names and trust', () => {
  it('never lets the proxy through to a name that answered public to the agent check and private afterwards', async () => {
    let answers = 0;
    const flipping: HostLookup = async () => [{ address: answers++ === 0 ? '93.184.216.34' : '127.0.0.1' }];
    const { browser } = await launched('full', flipping);
    const session = await browser.session();
    await openFor(browser, 'http://rebind.example/');
    expect((await viaProxy(session.chrome!.proxy!.url, 'http://rebind.example/')).status).toBe(403);
  }, 30_000);

  it('trusts the proxy for rebinding and WebSockets: neither is reported from the page session', async () => {
    const { cdp, browser, audit } = await launched('full');
    const session = await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    // Chrome sees only the proxy's address behind it.
    page.emit('Network.responseReceived', { response: { url: 'https://cdn.example/x', remoteIPAddress: '127.0.0.1', remotePort: 443 } });
    page.emit('Network.webSocketCreated', { requestId: 'w', url: 'ws://127.0.0.1:4000/' });
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(cdp.requests.some((request) => request.startsWith('GET /json/close/'))).toBe(false);
    expect(session.warning).toBeUndefined();
    expect(audit()).toEqual([]);
  });
});
