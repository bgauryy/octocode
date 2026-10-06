import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NO_REPLY, startCdpStub, type CdpHandler, type CdpStub, type StubPage } from './fixtures/cdp-stub.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { BrowserTool, registerBrowserTool } = await import('../src/browser/tool.js');
const { sharedChromePort } = await import('../src/browser/cdp.js');

type Result = { content: Array<{ type: string; text?: string }> };
type Tool = { execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: { cwd: string }) => Promise<Result> };

const stubs: CdpStub[] = [];
const tools: InstanceType<typeof BrowserTool>[] = [];
const dirs: string[] = [];

beforeEach(() => launch.mockReset());

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(tools.splice(0).map((tool) => tool.close()));
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
  for (const dir of dirs.splice(0)) fs.rmSync(dir, { recursive: true, force: true });
});

/**
 * A page like Chrome's while a dialog is open: every evaluation hangs until the dialog is answered. `onNavigate` and
 * `pageText` script the page; a snapshot reads a fixed page.
 */
function chrome(options: { onNavigate?: (page: StubPage) => void; pageText?: (page: StubPage) => unknown } = {}) {
  let blocked = false;
  const handle: CdpHandler = (method, params, page) => {
    if (method === 'Page.handleJavaScriptDialog') {
      blocked = false;
      return {};
    }
    if (method === 'Runtime.evaluate') {
      if (blocked) return NO_REPLY;
      const expression = String(params['expression']);
      if (expression.includes('items.push')) return { result: { value: { title: 'Stub', url: 'https://example.com/', text: 'page', items: ['[1] button "Go"'] } } };
      if (expression.includes('innerText') && options.pageText) return options.pageText(page);
      if (expression.includes('navigator.userAgent')) return { result: { value: 'StubAgent/1.0' } };
      if (expression === 'document.readyState') return { result: { value: 'complete' } };
      return { result: { value: true } };
    }
    if (method === 'Page.navigate') {
      options.onNavigate?.(page);
      return { frameId: 'f', loaderId: 'l' };
    }
    return {};
  };
  const block = (page: StubPage, type = 'alert', message = 'Hi!') => {
    blocked = true;
    page.emit('Page.javascriptDialogOpening', { type, message, url: 'https://example.com/' });
  };
  return { handle, block };
}

async function setup(handle: CdpHandler) {
  const cdp = await startCdpStub({ handle });
  stubs.push(cdp);
  vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
  const browser = new BrowserTool();
  tools.push(browser);
  let tool: Tool | undefined;
  const pi = { registerTool: (definition: Tool) => (tool = definition), on: () => undefined } as unknown as ExtensionAPI;
  registerBrowserTool(pi, browser);
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-browser-dialog-'));
  dirs.push(cwd);
  const run = (params: Record<string, unknown>) => tool!.execute('id', params, undefined, undefined, { cwd });
  const text = async (params: Record<string, unknown>) => (await run(params)).content.map((part) => part.text ?? '').join('\n');
  return { cdp, cwd, run, text };
}

const timed = async <T>(work: Promise<T>): Promise<{ value: T; ms: number }> => {
  const started = Date.now();
  const value = await work;
  return { value, ms: Date.now() - started };
};

describe('browser: an open dialog ends waits promptly', () => {
  it('navigate returns the dialog notice within a few seconds when an alert() blocks the load', async () => {
    // The alert opens while the page loads, so no load event fires until it is answered (Chrome waits ~20s, then ~30s per evaluation).
    let block: (target: StubPage) => void = () => undefined;
    const page = chrome({ onNavigate: (target) => setTimeout(() => block(target), 20) });
    block = (target) => page.block(target);
    const { text } = await setup(page.handle);
    const { value, ms } = await timed(text({ action: 'navigate', url: 'https://example.com/' }));
    expect(value).toContain('A alert dialog is open: "Hi!"');
    expect(ms).toBeLessThan(3_000);
    expect(await text({ action: 'dialog', accept: true })).toBe('Accepted the alert dialog.');
    expect(await text({ action: 'snapshot' })).toContain('# Stub');
  }, 15_000);

  it('navigate with text stops waiting when a dialog opens after the load', async () => {
    let block: (target: StubPage) => void = () => undefined;
    const page = chrome({ onNavigate: (target) => setTimeout(() => target.emit('Page.loadEventFired'), 5), pageText: (target) => (block(target), NO_REPLY) });
    block = (target) => page.block(target, 'confirm', 'Stay?');
    const { text } = await setup(page.handle);
    const { value, ms } = await timed(text({ action: 'navigate', url: 'https://example.com/', text: 'Ready', timeout: 20 }));
    expect(value).toContain('A confirm dialog is open: "Stay?"');
    expect(ms).toBeLessThan(3_000);
  }, 15_000);

  it('wait reports a dialog that opens mid-wait at once', async () => {
    let polls = 0;
    let block: (target: StubPage) => void = () => undefined;
    const page = chrome({
      pageText: (target) => {
        if (++polls < 3) return { result: { value: 'loading' } };
        block(target);
        return NO_REPLY;
      },
    });
    block = (target) => page.block(target, 'prompt', 'Name?');
    const { text } = await setup(page.handle);
    const { value, ms } = await timed(text({ action: 'wait', text: 'Ready', timeout: 20 }));
    expect(value).toContain('A prompt dialog is open: "Name?"');
    expect(ms).toBeLessThan(3_000);
  }, 15_000);

  it('wait does not overrun its timeout when the page stops answering', async () => {
    const page = chrome({ pageText: () => NO_REPLY });
    const { run } = await setup(page.handle);
    const started = Date.now();
    await expect(run({ action: 'wait', text: 'Ready', timeout: 1 })).rejects.toThrow('Timed out after 1s waiting for "Ready" to appear.');
    expect(Date.now() - started).toBeLessThan(2_500);
  }, 15_000);

  it('a timed-out wait fails with the current page attached', async () => {
    const page = chrome({ pageText: () => 'Still loading' });
    const { run } = await setup(page.handle);
    await expect(run({ action: 'wait', text: 'Ready', timeout: 1 })).rejects.toThrow(/Timed out after 1s waiting for "Ready" to appear\.\n\nCurrent page:\n/);
  }, 15_000);
});

describe('browser: every page the session switches to gets the same setup', () => {
  it('applies the download folder and the locale to a tab it opens', async () => {
    vi.stubEnv('OCTOCODE_BROWSER_LOCALE', 'de-DE');
    const { cdp, cwd, text } = await setup(chrome().handle);
    expect(await text({ action: 'download' })).toContain('No downloads yet');
    await text({ action: 'tab', url: 'https://example.com/other' });
    const onTab = cdp.commands.filter((command) => command.target === 'tab-2').map((command) => command.method);
    expect(onTab).toContain('Emulation.setLocaleOverride');
    const downloads = cdp.commands.filter((command) => command.target === 'tab-2' && command.method === 'Browser.setDownloadBehavior');
    expect(downloads.map((command) => command.params)).toEqual([{ behavior: 'allow', downloadPath: path.join(cwd, 'downloads'), eventsEnabled: true }]);
  }, 15_000);

  it('leaves downloads alone on a tab switch before download was used', async () => {
    const { cdp, text } = await setup(chrome().handle);
    await text({ action: 'tab', url: 'https://example.com/other' });
    expect(cdp.commands.some((command) => command.method.endsWith('setDownloadBehavior'))).toBe(false);
  }, 15_000);
});

describe('sharedChromePort', () => {
  it('shares an open Chrome only on an explicit OCTOCODE_CHROME_PORT', () => {
    expect(sharedChromePort({})).toBeUndefined();
    expect(sharedChromePort({ OCTOCODE_CHROME_PORT: '' })).toBeUndefined();
    expect(sharedChromePort({ OCTOCODE_CHROME_PORT: ' 9333 ' })).toBe(9333);
    expect(sharedChromePort({ OCTOCODE_CHROME_PORT: 'abc' })).toBe(9222);
    expect(sharedChromePort({ OCTOCODE_CHROME_PORT: '0' })).toBe(1);
    expect(sharedChromePort({ OCTOCODE_CHROME_PORT: '99999' })).toBe(65_535);
  });
});
