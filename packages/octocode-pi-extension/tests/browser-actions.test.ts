import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NO_REPLY, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { BrowserTool, registerBrowserTool } = await import('../src/browser/tool.js');

type Result = { content: Array<{ type: string; text?: string }> };
type Tool = { execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: { cwd: string }) => Promise<Result> };

const stubs: CdpStub[] = [];
const tools: InstanceType<typeof BrowserTool>[] = [];
const dirs: string[] = [];

const tmp = (): string => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-browser-actions-'));
  dirs.push(dir);
  return dir;
};

beforeEach(() => launch.mockReset());

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(tools.splice(0).map((tool) => tool.close()));
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
  for (const dir of dirs.splice(0)) fs.rmSync(dir, { recursive: true, force: true });
});

/** What each page action does: `click` runs when element [ref] is clicked (by the click action or by upload/download). */
function handler(onClick: (ref: string, page: Parameters<CdpHandler>[2]) => unknown = () => ({ result: { value: true } }), extra: CdpHandler = () => ({})): CdpHandler {
  // The element last measured; the mouse press that follows is what clicks it, as in Chrome.
  let pointed = '';
  return (method, params, page) => {
    if (method === 'Input.dispatchMouseEvent' && params['type'] === 'mousePressed') return onClick(pointed, page) === NO_REPLY ? NO_REPLY : {};
    if (method === 'Runtime.evaluate') {
      const expression = String(params['expression']);
      if (expression.includes('items.push')) return { result: { value: { title: 'Stub', url: 'https://example.com/', text: 'page', items: ['[1] input:file "File"', '[2] button "Upload"'] } } };
      const index = /__octoRefs \|\| \[\]\)\[(\d+)\]/.exec(expression)?.[1];
      const ref = index === undefined ? '' : String(Number(index) + 1);
      // A click reads the element's centre, then presses the mouse there.
      if (expression.includes('elementFromPoint')) {
        pointed = ref;
        return { result: { value: { x: 5, y: 5, covered: '' } } };
      }
      if (expression.includes('document.readyState')) return { result: { value: true } };
      if (expression.includes('location.href')) return { result: { value: 'https://example.com/ | Stub' } };
      // uploadFiles resolves the element to a remote object.
      if (ref) return { result: { type: 'object', objectId: `obj-${ref}` } };
      return { result: { value: 1 } };
    }
    if (method === 'Runtime.callFunctionOn') {
      const body = String(params['functionDeclaration']);
      if (body.includes("this.type === 'file'")) return { result: { value: params['objectId'] === 'obj-1' } };
      if (body.includes('this.click()')) setTimeout(() => page.emit('Page.fileChooserOpened', { frameId: 'f', mode: 'selectSingle', backendNodeId: 77 }), 5);
      return { result: {} };
    }
    return extra(method, params, page);
  };
}

async function setup(handle: CdpHandler) {
  const cdp = await startCdpStub({ handle });
  stubs.push(cdp);
  vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
  return { cdp, ...register() };
}

function register() {
  const browser = new BrowserTool();
  tools.push(browser);
  let tool: Tool | undefined;
  const handlers = new Map<string, () => Promise<void>>();
  const pi = { registerTool: (definition: Tool) => (tool = definition), on: (event: string, fn: () => Promise<void>) => handlers.set(event, fn) } as unknown as ExtensionAPI;
  registerBrowserTool(pi, browser);
  const cwd = tmp();
  const run = (params: Record<string, unknown>) => tool!.execute('id', params, undefined, undefined, { cwd });
  const text = async (params: Record<string, unknown>) => (await run(params)).content.map((part) => part.text ?? '').join('\n');
  return { browser, handlers, cwd, run, text };
}

describe('browser dialogs', () => {
  it('reports an open dialog instead of hanging, and answers it', async () => {
    const { cdp, run, text } = await setup(
      handler((_ref, page) => {
        page.emit('Page.javascriptDialogOpening', { type: 'confirm', message: 'Delete it?', url: 'https://example.com/' });
        return NO_REPLY; // the click blocks while the dialog is open, like Chrome
      }),
    );
    await expect(run({ action: 'dialog' })).rejects.toThrow('No dialog is open.');
    const clicked = await text({ action: 'click', ref: 2 });
    expect(clicked).toContain('A confirm dialog is open: "Delete it?"');
    for (const action of ['snapshot', 'evaluate', 'press']) expect(await text({ action, expression: '1' }), action).toContain('dialog is open');
    expect(await text({ action: 'info' })).toContain('dialog is open');

    expect(await text({ action: 'dialog', accept: false })).toBe('Dismissed the confirm dialog.');
    expect(cdp.commands.find((command) => command.method === 'Page.handleJavaScriptDialog')?.params).toEqual({ accept: false });
    expect(await text({ action: 'snapshot' })).toContain('# Stub');
  }, 20_000);

  it('passes the prompt reply, and forgets a dialog the user closed', async () => {
    const { cdp, text } = await setup(
      handler((_ref, page) => {
        page.emit('Page.javascriptDialogOpening', { type: 'prompt', message: 'Name?' });
        return NO_REPLY;
      }),
    );
    await text({ action: 'click', ref: 2 });
    expect(await text({ action: 'dialog', text: 'Ada' })).toBe('Accepted the prompt dialog.');
    expect(cdp.commands.find((command) => command.method === 'Page.handleJavaScriptDialog')?.params).toEqual({ accept: true, promptText: 'Ada' });

    await text({ action: 'click', ref: 2 });
    cdp.pages.values().next().value!.emit('Page.javascriptDialogClosed', { result: true });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(await text({ action: 'snapshot' })).toContain('# Stub');
  }, 20_000);
});

describe('browser upload', () => {
  it('sets files on a file input, or through the chooser an upload button opens', async () => {
    const { cdp, cwd, run, text } = await setup(handler());
    fs.writeFileSync(path.join(cwd, 'a.txt'), 'a');
    await expect(run({ action: 'upload', ref: 1 })).rejects.toThrow(/upload needs files/);
    await expect(run({ action: 'upload', ref: 1, files: ['missing.txt'] })).rejects.toThrow(`No such file: ${path.join(cwd, 'missing.txt')}`);

    expect(await text({ action: 'upload', ref: 1, files: ['a.txt'] })).toBe('Attached to [1]: a.txt');
    expect(cdp.commands.find((command) => command.method === 'DOM.setFileInputFiles')?.params).toEqual({ files: [path.join(cwd, 'a.txt')], objectId: 'obj-1' });

    expect(await text({ action: 'upload', ref: 2, files: [path.join(cwd, 'a.txt')] })).toBe('Attached to [2]: a.txt');
    const methods = cdp.commands.map((command) => command.method);
    expect(methods).toContain('Page.setInterceptFileChooserDialog');
    expect(cdp.commands.filter((command) => command.method === 'DOM.setFileInputFiles').at(-1)?.params).toEqual({ files: [path.join(cwd, 'a.txt')], backendNodeId: 77 });
    // Interception is turned back off.
    expect(cdp.commands.filter((command) => command.method === 'Page.setInterceptFileChooserDialog').map((command) => command.params['enabled'])).toEqual([true, false]);
  }, 20_000);
});

describe('browser download', () => {
  it('turns downloads on, clicks, waits for the file and lists it', async () => {
    const { cdp, cwd, text } = await setup(
      handler((_ref, page) => {
        setTimeout(() => page.emit('Browser.downloadWillBegin', { guid: 'g1', suggestedFilename: 'report.csv', url: 'https://example.com/r' }), 5);
        setTimeout(() => page.emit('Browser.downloadProgress', { guid: 'g1', state: 'inProgress', receivedBytes: 4, totalBytes: 12 }), 10);
        setTimeout(() => page.emit('Browser.downloadProgress', { guid: 'g1', state: 'completed', receivedBytes: 12, totalBytes: 12 }), 150);
        return { result: { value: true } };
      }),
    );
    const dir = path.join(cwd, 'downloads');
    expect(await text({ action: 'download' })).toBe(`No downloads yet (saving to ${dir}). Pass ref to click a download link.`);
    expect(cdp.commands.find((command) => command.method === 'Browser.setDownloadBehavior')?.params).toEqual({ behavior: 'allow', downloadPath: dir, eventsEnabled: true });
    expect(fs.existsSync(dir)).toBe(true);
    expect(await text({ action: 'download', ref: 2 })).toBe(`Downloads in ${dir}:\n- report.csv · 12 bytes · saved`);
    // Turned on once per session.
    expect(cdp.commands.filter((command) => command.method === 'Browser.setDownloadBehavior')).toHaveLength(1);
  }, 20_000);

  it('falls back to the page-level download switch', async () => {
    const { cdp, text } = await setup(handler(undefined, (method) => (method === 'Browser.setDownloadBehavior' ? { error: { message: 'not allowed' } } : {})));
    expect(await text({ action: 'download' })).toContain('No downloads yet');
    expect(cdp.commands.map((command) => command.method)).toContain('Page.setDownloadBehavior');
  }, 20_000);
});

describe('visible mode (webLive)', () => {
  async function deadPort(): Promise<number> {
    const cdp = await startCdpStub();
    const port = cdp.port;
    await cdp.close();
    return port;
  }

  it('launches a visible Chrome on the persistent profile and leaves it open at shutdown', async () => {
    const home = tmp();
    const chrome = await startCdpStub({ handle: handler() });
    stubs.push(chrome);
    const kill = vi.fn(async () => undefined);
    const unref = vi.fn();
    launch.mockResolvedValue({ port: chrome.port, kill, process: { unref } });
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { handlers, text } = register();

    const profile = path.join(home, 'pi-browser-profile');
    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    expect(await text({ action: 'info' })).toBe(`own Chrome (visible) · profile ${profile} · DevTools port ${chrome.port} · https://example.com/ | Stub`);
    expect(launch.mock.calls[0]![0]).toMatchObject({ userDataDir: profile });
    expect(launch.mock.calls[0]![0].chromeFlags).not.toContain('--headless=new');
    await handlers.get('session_shutdown')!();
    expect(kill).not.toHaveBeenCalled();
    expect(unref).toHaveBeenCalled();
    // Chrome writes DevToolsActivePort only for port 0, so the launch records it for the next run.
    expect(fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8')).toBe(`${chrome.port}\n`);

    // The next session reuses that window; an explicit close quits it.
    await text({ action: 'snapshot' });
    expect(await text({ action: 'info' })).toMatch(/^shared Chrome, own tab · profile /);
    expect(launch).toHaveBeenCalledTimes(1);
    await text({ action: 'close' });
    expect(chrome.commands.some((command) => command.method === 'Browser.close')).toBe(true);
    expect(kill).not.toHaveBeenCalled();
  }, 20_000);

  it('kills a launched profile Chrome on an explicit close', async () => {
    const chrome = await startCdpStub({ handle: handler() });
    stubs.push(chrome);
    const kill = vi.fn(async () => undefined);
    launch.mockResolvedValue({ port: chrome.port, kill, process: { unref: vi.fn() } });
    vi.stubEnv('OCTOCODE_HOME', tmp());
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { text } = register();
    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    await text({ action: 'info' });
    await text({ action: 'close' });
    expect(kill).toHaveBeenCalledTimes(1);
  }, 20_000);

  it('reuses the profile Chrome from its DevToolsActivePort, keeping its tab at shutdown', async () => {
    const home = tmp();
    const running = await startCdpStub({ handle: handler() });
    stubs.push(running);
    const profile = path.join(home, 'pi-browser-profile');
    fs.mkdirSync(profile, { recursive: true });
    fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${running.port}\n/devtools/browser/x\n`);
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { handlers, text } = register();

    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    expect(await text({ action: 'info' })).toBe(`shared Chrome, own tab · profile ${profile} · DevTools port ${running.port} · https://example.com/ | Stub`);
    expect(launch).not.toHaveBeenCalled();
    expect(running.requests).toContain('PUT /json/new?about:blank');
    await handlers.get('session_shutdown')!();
    expect(running.requests.some((request) => request.startsWith('GET /json/close'))).toBe(false);
  }, 20_000);

  it('refuses a profile another Chrome holds without DevTools', async () => {
    const home = tmp();
    const profile = path.join(home, 'pi-browser-profile');
    fs.mkdirSync(profile, { recursive: true });
    fs.symlinkSync(`${os.hostname()}-${process.pid}`, path.join(profile, 'SingletonLock'));
    // A stale port file: nothing answers there.
    fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${await deadPort()}\n`);
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { run } = register();
    await expect(run({ action: 'snapshot' })).rejects.toThrow(`The browser profile ${profile} is open in a Chrome without DevTools. Close that Chrome and retry.`);
    expect(launch).not.toHaveBeenCalled();
  }, 20_000);
});

describe('browser actions without a browser', async () => {
  const { answerDialog, download, uploadFiles } = await import('../src/browser/actions.js');
  type Session = Parameters<typeof download>[0];
  type Sent = { method: string; params: Record<string, unknown> };

  /** A page that answers `send` from `reply`, records it, and lets the test emit events. */
  function fakeSession(reply: (method: string, params: Record<string, unknown>) => unknown = () => ({})) {
    const listeners = new Set<(method: string, params: Record<string, unknown>) => void>();
    const sent: Sent[] = [];
    const page = {
      on: (listener: (method: string, params: Record<string, unknown>) => void) => (listeners.add(listener), () => listeners.delete(listener)),
      send: async (method: string, params: Record<string, unknown> = {}) => (sent.push({ method, params }), reply(method, params) ?? {}),
    };
    const session = { page, downloads: new Map(), console: [] } as unknown as Session;
    return { session, page: session.page, sent, listeners };
  }
  const element = 'document.body';

  afterEach(() => vi.useRealTimers());

  it('refuses to answer when no dialog is open, and passes the prompt reply when one is', async () => {
    const { session, sent } = fakeSession();
    await expect(answerDialog(session, true, undefined)).rejects.toThrow('No dialog is open.');
    session.dialog = { type: 'prompt', message: 'Name?' };
    expect(await answerDialog(session, false, 'Ada')).toBe('Dismissed the prompt dialog.');
    expect(sent).toEqual([{ method: 'Page.handleJavaScriptDialog', params: { accept: false, promptText: 'Ada' } }]);
    expect(session.dialog).toBeUndefined();
  });

  it('refuses an upload with no files, a missing file, or a gone element', async () => {
    const dir = tmp();
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const { page } = fakeSession(() => ({ result: {} }));
    await expect(uploadFiles(page, 1, element, [], dir)).rejects.toThrow('upload needs files');
    await expect(uploadFiles(page, 1, element, ['missing.txt'], dir)).rejects.toThrow(`No such file: ${path.join(dir, 'missing.txt')}`);
    await expect(uploadFiles(page, 1, element, ['a.txt'], dir)).rejects.toThrow('No element [1] — take a new snapshot.');
  });

  /** A non-input element: clicking it may or may not open a file chooser. */
  const button = (method: string) => (method === 'Runtime.evaluate' ? { result: { objectId: 'obj' } } : method === 'Runtime.callFunctionOn' ? { result: { value: false } } : {});

  it('says when clicking an upload button opened no file chooser, and turns interception back off', async () => {
    const dir = tmp();
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const { page, sent } = fakeSession(button);
    vi.useFakeTimers();
    const upload = expect(uploadFiles(page, 2, element, ['a.txt'], dir)).rejects.toThrow('Clicking [2] opened no file chooser; use the file input instead.');
    await vi.advanceTimersByTimeAsync(5_000);
    await upload;
    expect(sent.filter((item) => item.method === 'Page.setInterceptFileChooserDialog').map((item) => item.params['enabled'])).toEqual([true, false]);
  });

  it('stops waiting for the file chooser when aborted, before or during the wait', async () => {
    const dir = tmp();
    fs.writeFileSync(path.join(dir, 'a.txt'), 'a');
    const before = new AbortController();
    before.abort(new Error('already stopped'));
    const first = fakeSession(button);
    await expect(uploadFiles(first.page, 2, element, ['a.txt'], dir, before.signal)).rejects.toThrow('already stopped');
    const during = new AbortController();
    const second = fakeSession((method) => {
      if (method === 'Runtime.callFunctionOn' && second.sent.filter((item) => item.method === method).length === 2) setTimeout(() => during.abort(), 5);
      return button(method);
    });
    await expect(uploadFiles(second.page, 2, element, ['a.txt'], dir, during.signal)).rejects.toThrow(/aborted/i);
    expect(second.listeners.size).toBe(0);
    expect(second.sent.at(-1)).toEqual({ method: 'Page.setInterceptFileChooserDialog', params: { enabled: false } });
  });

  it('turns downloads on once and says when there are none yet', async () => {
    const cwd = tmp();
    const { session, sent } = fakeSession();
    expect(await download(session, cwd, undefined)).toBe(`No downloads yet (saving to ${path.join(cwd, 'downloads')}). Pass ref to click a download link.`);
    expect(fs.statSync(path.join(cwd, 'downloads')).isDirectory()).toBe(true);
    expect(session.downloadDir).toBe(path.join(cwd, 'downloads'));
    await download(session, cwd, undefined);
    expect(sent.map((item) => item.method)).toEqual(['Browser.setDownloadBehavior']);
  });

  it('stops waiting for a running download when aborted, and lists what finished', async () => {
    const cwd = tmp();
    const { session } = fakeSession();
    session.downloads.set('g', { name: 'big.zip', state: 'inProgress', bytes: 1 });
    const controller = new AbortController();
    setTimeout(() => controller.abort(new Error('enough')), 20);
    await expect(download(session, cwd, undefined, controller.signal)).rejects.toThrow('enough');
    session.downloads.set('g', { name: 'big.zip', state: 'canceled', bytes: 1 });
    session.downloads.set('h', { name: 'ok.csv', state: 'completed', bytes: 12 });
    expect(await download(session, cwd, undefined)).toBe(`Downloads in ${path.join(cwd, 'downloads')}:\n- big.zip · 1 bytes · canceled\n- ok.csv · 12 bytes · saved`);
  });
});
