import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fakeCtx } from './fake-pi.js';
import { answered, openFor, paused, PRIVATE_HOSTS, useNetworkHarness } from './browser-network-harness.js';
import { PagePolicy, VERDICTS_MAX } from '../src/browser/network.js';
import type { HostLookup } from '../src/web/guard.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { AUDIT_KEYS_MAX } = await import('../src/browser/tool.js');
const { setup } = useNetworkHarness();

beforeEach(() => launch.mockReset());

describe('browser page requests', () => {
  it('pauses every resource type with the cache and service workers off', async () => {
    const { cdp, browser } = await setup();
    await browser.session();
    const methods = cdp.commands.map((command) => command.method);
    expect(cdp.commands.find((command) => command.method === 'Fetch.enable')?.params).toEqual({ patterns: [{ urlPattern: '*', requestStage: 'Request' }] });
    expect(cdp.commands.find((command) => command.method === 'Network.setCacheDisabled')?.params).toEqual({ cacheDisabled: true });
    expect(cdp.commands.find((command) => command.method === 'Network.setBypassServiceWorker')?.params).toEqual({ bypass: true });
    // A service worker would answer a page's fetch unpaused, so the bypass is in place before the first pause.
    expect(methods.indexOf('Network.setBypassServiceWorker')).toBeLessThan(methods.indexOf('Fetch.enable'));
    expect(methods.indexOf('Network.enable')).toBeLessThan(methods.indexOf('Fetch.enable'));
  });

  it('fails fetch, image, subframe, redirect-hop and worker requests to private hosts, and passes public ones', async () => {
    const { cdp, browser, audit } = await setup();
    const session = await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    const types: Array<[string, string, Record<string, unknown>?]> = [
      ['XHR', PRIVATE_HOSTS[0]!],
      ['Image', PRIVATE_HOSTS[1]!],
      ['Document', PRIVATE_HOSTS[2]!],
      ['XHR', PRIVATE_HOSTS[3]!, { redirectedRequestId: 'hop' }],
      ['Other', PRIVATE_HOSTS[4]!],
      ['Script', 'http://localhost:1/x.js'],
    ];
    types.forEach(([type, url, extra], index) => page.emit('Fetch.requestPaused', paused(`bad-${index}`, url, type, extra)));
    page.emit('Fetch.requestPaused', paused('good-1', 'https://93.184.216.34/app.js', 'Script'));
    page.emit('Fetch.requestPaused', paused('good-2', 'data:image/png;base64,AAAA', 'Image'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toHaveLength(6));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest').sort()).toEqual(['good-1', 'good-2']));
    expect(answered(cdp, 'Fetch.failRequest').sort()).toEqual(['bad-0', 'bad-1', 'bad-2', 'bad-3', 'bad-4', 'bad-5']);
    expect(cdp.commands.filter((command) => command.method === 'Fetch.failRequest').every((command) => command.params['errorReason'] === 'BlockedByClient')).toBe(true);
    // The agent is told, and the console keeps the evidence.
    expect(session.blocked).toMatch(/Blocked (private or local address|unresolved host)/);
    expect(session.console.some((line) => line.startsWith('[blocked] http://10.0.0.1/admin: Blocked private or local address: 10.0.0.1'))).toBe(true);
    // No dialog and no ask: only the audit, one entry per host.
    const refused = audit().filter((entry) => entry.action === 'invariant');
    expect(refused.map((entry) => entry.allowed)).toEqual(refused.map(() => false));
    expect(refused).toHaveLength(6);
    expect(refused[0]!.note).toMatch(/^browser: blocked a page request \(XHR\)/);
    // The same refusal again is counted, not written again; a different resource type to the same host is another refusal and is audited.
    page.emit('Fetch.requestPaused', paused('again', PRIVATE_HOSTS[0]!, 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toContain('again'));
    expect(audit().filter((entry) => entry.action === 'invariant')).toHaveLength(6);
    page.emit('Fetch.requestPaused', paused('other-type', PRIVATE_HOSTS[0]!, 'Image'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toContain('other-type'));
    expect(audit().filter((entry) => entry.action === 'invariant')).toHaveLength(7);
    expect(audit().filter((entry) => entry.action === 'invariant').at(-1)!.note).toMatch(/blocked a page request \(Image\)/);
    // When the browser session ends, the repeat is audited with its count.
    await browser.close();
    const summary = audit().filter((entry) => entry.action === 'invariant').at(-1)!;
    expect(summary.note).toMatch(/^browser: \(2 times this browser session\) blocked a page request \(XHR\)/);
  });

  it('audits a repeat count soon after it grows, not only when the browser session ends', async () => {
    const { cdp, browser, audit } = await setup({ flushMs: 40 });
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    for (const id of ['r1', 'r2', 'r3']) page.emit('Fetch.requestPaused', paused(id, 'http://127.0.0.1:9/x', 'XHR'));
    const notes = () => audit().filter((entry) => entry.action === 'invariant').map((entry) => entry.note);
    // Written before any close: a crash now still leaves the count.
    await vi.waitFor(() => expect(notes().at(-1)).toMatch(/^browser: \(3 times this browser session\) blocked a page request/));
    expect(notes()).toHaveLength(2);
    // Nothing new, nothing more written; a further repeat is written again, cumulatively.
    await new Promise((resolve) => setTimeout(resolve, 120));
    expect(notes()).toHaveLength(2);
    page.emit('Fetch.requestPaused', paused('r4', 'http://127.0.0.1:9/x', 'XHR'));
    await vi.waitFor(() => expect(notes().at(-1)).toMatch(/^browser: \(4 times this browser session\)/));
    await browser.close();
    expect(notes()).toHaveLength(3);
  });

  it('counts the refusals past the distinct-entry bound under one entry instead of listing or dropping them', async () => {
    const { cdp, browser, audit } = await setup();
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    const total = AUDIT_KEYS_MAX + 7;
    for (let n = 0; n < total; n++) page.emit('Fetch.requestPaused', paused(`r${n}`, `http://10.0.${n >> 8}.${n & 255}/`, 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toHaveLength(total), { timeout: 5_000 });
    const entries = () => audit().filter((entry) => entry.action === 'invariant');
    await vi.waitFor(() => expect(entries()).toHaveLength(AUDIT_KEYS_MAX + 1));
    expect(entries().at(-1)!.note).toMatch(/^browser: 1 more distinct refusals or reports after the first 100/);
    await browser.close();
    expect(entries()).toHaveLength(AUDIT_KEYS_MAX + 2);
    expect(entries().at(-1)!.note).toMatch(/^browser: 7 more distinct refusals or reports after the first 100, counted here and not listed one by one/);
  });

  it('bounds the host verdicts it keeps: the oldest is dropped at the bound and classified again, never answered wrongly', async () => {
    const names = Array.from({ length: VERDICTS_MAX + 1 }, (_, n) => `h${n}.example`);
    const counting = vi.fn(async () => [{ address: '93.184.216.34' }]);
    const bounded = new PagePolicy({ report: () => undefined, lookup: counting, now: () => 0 });
    for (const name of names) await bounded.check(`https://${name}/`, 'Image');
    expect(counting).toHaveBeenCalledTimes(names.length);
    // The newest is still remembered; the oldest was dropped at the bound and is looked up again.
    await bounded.check(`https://${names.at(-1)}/`, 'Image');
    expect(counting).toHaveBeenCalledTimes(names.length);
    await bounded.check(`https://${names[0]}/`, 'Image');
    expect(counting).toHaveBeenCalledTimes(names.length + 1);
  });

  it('does not approve a name that answered public to the agent check and private afterwards, and never grants net.private for it', async () => {
    for (const mode of ['full', 'workspace'] as const) {
      let answers = 0;
      const flipping: HostLookup = async () => [{ address: answers++ === 0 ? '93.184.216.34' : '10.0.0.5' }];
      const { cdp, browser, audit } = await setup({ mode, lookup: flipping });
      const ctx = fakeCtx({ cwd: '/private/tmp' });
      const select = vi.fn(async (_title: string, choices: string[]) => choices[0]!);
      ctx.ui.select = select;
      browser.useContext(ctx, 'call-1');
      await browser.session();
      const page = [...cdp.pages.values()].at(-1)!;
      await browser.open('http://rebind.example/');
      // The page's own requests to that name now see a private answer: refused, though the agent opened the name.
      page.emit('Fetch.requestPaused', paused('own', 'http://rebind.example/app.js', 'Script'));
      await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['own']));
      expect(answered(cdp, 'Fetch.continueRequest')).toEqual([]);
      expect(audit().filter((entry) => entry.action === 'net.private')).toEqual([]);
      expect(select).not.toHaveBeenCalled();
      await browser.close();
    }
  });

  it('still asks net.private for a private name and approves it once granted', async () => {
    const privateLookup: HostLookup = async () => [{ address: '10.0.0.5' }];
    const { cdp, browser, audit } = await setup({ mode: 'full', lookup: privateLookup });
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    await openFor(browser, 'http://app.local:8080/');
    expect(audit().filter((entry) => entry.action === 'net.private')).toHaveLength(1);
    page.emit('Fetch.requestPaused', paused('own', 'http://app.local:8080/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toEqual(['own']));
    // Another port of the same name stays the page's own choice.
    page.emit('Fetch.requestPaused', paused('other', 'http://app.local:9090/', 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['other']));
  });

  it('forgets what the agent opened when the browser session ends', async () => {
    const privateLookup: HostLookup = async () => [{ address: '10.0.0.5' }];
    const { cdp, browser } = await setup({ mode: 'full', lookup: privateLookup });
    await browser.session();
    await openFor(browser, 'http://app.local:8080/');
    await browser.close();
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    page.emit('Fetch.requestPaused', paused('again', 'http://app.local:8080/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['again']));
  });

  it('fails private page requests in every mode, with nothing to confirm', async () => {
    for (const mode of ['manual', 'workspace', 'full'] as const) {
      const { cdp, browser, audit } = await setup({ mode });
      await browser.session();
      [...cdp.pages.values()].at(-1)!.emit('Fetch.requestPaused', paused('r', 'http://127.0.0.1:9/', 'Image'));
      await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['r']), { timeout: 5_000 });
      expect(audit().filter((entry) => entry.action === 'invariant')).toHaveLength(1);
    }
  });

  it('lets the page load from the target the agent opened, and nothing else private', async () => {
    const { cdp, browser } = await setup({ mode: 'workspace' });
    const ctx = fakeCtx({ cwd: '/private/tmp' });
    ctx.ui.select = async (_title, choices) => choices.find((choice) => choice.startsWith('Allow')) ?? choices[0]!;
    browser.useContext(ctx, 'call-1');
    await browser.session();
    const page = [...cdp.pages.values()].at(-1)!;
    page.emit('Fetch.requestPaused', paused('before', 'http://127.0.0.1:3000/app.js', 'Script'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['before']));
    await openFor(browser, 'http://127.0.0.1:3000/');
    page.emit('Fetch.requestPaused', paused('own', 'http://127.0.0.1:3000/app.js', 'Script'));
    page.emit('Fetch.requestPaused', paused('other', 'http://127.0.0.1:3001/api', 'XHR'));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.continueRequest')).toEqual(['own']));
    await vi.waitFor(() => expect(answered(cdp, 'Fetch.failRequest')).toEqual(['before', 'other']));
  });

});
