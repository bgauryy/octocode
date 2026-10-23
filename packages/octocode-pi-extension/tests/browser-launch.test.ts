import { beforeEach, describe, expect, it, vi } from 'vitest';
import { startCdpStub } from './fixtures/cdp-stub.js';
import { useBrowserHarness } from './browser-harness.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));
vi.mock('node:dns/promises', () => ({ lookup: async () => [{ address: '93.184.216.34', family: 4 }] })); // public: no network

const { openBrowser } = await import('../src/browser/session.js');
const { stub, register } = useBrowserHarness();

beforeEach(() => {
  launch.mockReset();
});

async function closedPort(): Promise<number> {
  const cdp = await startCdpStub();
  await cdp.close();
  return cdp.port;
}

describe('browser launching its own Chrome', () => {
  it('launches headless Chrome when none is listening, and kills it on close', async () => {
    const chrome = await stub();
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: chrome.port, kill });
    const session = await openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) });
    expect(session).toMatchObject({ kind: 'own', headless: true, port: chrome.port });
    expect(launch.mock.calls[0][0].chromeFlags).toContain('--headless=new');
    const { browser } = register();
    session.page.close();

    // No knob shows the temporary-profile Chrome: only visible mode's persistent profile opens a window.
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await closedPort()));
    vi.stubEnv('OCTOCODE_BROWSER_HEADLESS', '0');
    const own = await browser.session();
    expect(own.headless).toBe(true);
    expect(launch.mock.calls[1][0].chromeFlags).toContain('--headless=new');
    await browser.close();
    expect(kill).toHaveBeenCalledTimes(1);
  }, 20_000);

  it('falls back to launching when the running Chrome refuses a new tab', async () => {
    const refusing = await stub({ newTabStatus: 500 });
    const chrome = await stub();
    launch.mockResolvedValue({ port: chrome.port, kill: vi.fn(async () => undefined) });
    const session = await openBrowser({ OCTOCODE_CHROME_PORT: String(refusing.port) });
    expect(session.kind).toBe('own');
    session.page.close();
  });

  it('stops a launch the tool call aborted: rejects at once and kills the Chrome once it comes up', async () => {
    const chrome = await stub();
    const kill = vi.fn(async () => undefined);
    let launched!: (value: unknown) => void;
    launch.mockReturnValue(new Promise((resolve) => (launched = resolve)));
    const controller = new AbortController();
    const opening = openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) }, { signal: controller.signal });
    await vi.waitFor(() => expect(launch).toHaveBeenCalledTimes(1));
    controller.abort(new Error('Stopped by the user'));
    await expect(opening).rejects.toThrow('Stopped by the user');
    launched({ port: chrome.port, kill });
    await vi.waitFor(() => expect(kill).toHaveBeenCalledTimes(1));

    // The tool passes its call's signal: an aborted call opens nothing.
    const { browser } = register();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await closedPort()));
    await expect(browser.session(AbortSignal.abort(new Error('Stopped early')))).rejects.toThrow('Stopped early');
    expect(launch).toHaveBeenCalledTimes(1);
  });

  it('kills the launched Chrome when its page cannot be reached', async () => {
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: await closedPort(), kill });
    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) })).rejects.toThrow();
    expect(kill).toHaveBeenCalledTimes(1);
  });

  it('kills the launched Chrome, and closes a tab it opened, when the page session fails to start', async () => {
    const failing = await stub({ handle: (method) => (method === 'Page.enable' ? { error: { message: 'Page domain unavailable' } } : {}) });
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: failing.port, kill });
    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(await closedPort()) })).rejects.toThrow('Page domain unavailable');
    expect(kill).toHaveBeenCalledTimes(1);

    await expect(openBrowser({ OCTOCODE_CHROME_PORT: String(failing.port) })).rejects.toThrow('Page domain unavailable');
    await vi.waitFor(() => expect(failing.requests.some((request) => request.startsWith('GET /json/close/tab-'))).toBe(true));
  });
});

describe('CDP page session', () => {
  it('ignores malformed messages and throwing listeners, and surfaces command errors', async () => {
    const cdp = await stub();
    const session = await openBrowser({}, { port: cdp.port });
    const page = cdp.pages.get('tab-1')!;
    const off = session.page.on(() => {
      throw new Error('listener bug');
    });
    page.raw('not json');
    page.emit('Some.event');
    await expect(session.page.send('Fail.always')).rejects.toThrow('no such method');
    off();
    const waited = session.page.waitFor('Never.fired', 20);
    await expect(waited).resolves.toBeUndefined();
    await session.closeTab?.();
    session.page.close();
  });

  it('stops waiting for commands and events when the tool call is aborted', async () => {
    const cdp = await stub();
    const session = await openBrowser({}, { port: cdp.port });
    const controller = new AbortController();
    const pending = session.page.send('Slow.never', {}, controller.signal);
    const waiting = session.page.waitFor('Never.fired', 60_000, controller.signal);
    controller.abort(new Error('user cancelled'));
    await expect(pending).rejects.toThrow('user cancelled');
    await expect(waiting).rejects.toThrow('user cancelled');
    await expect(session.page.evaluate('1', controller.signal)).rejects.toThrow('user cancelled');
    await session.closeTab?.();
    session.page.close();
  });
});
