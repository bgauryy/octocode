import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NO_REPLY, startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';
import { fakePermission } from './fake-pi.js';
import { tmp } from './helpers.js';
import type { PermissionMode } from '../src/permission/mode.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));

const { BrowserTool, registerBrowserTool } = await import('../src/browser/tool.js');
const { sessionOutputDir } = await import('../src/shared/home.js');

type Result = { content: Array<{ type: string; text?: string }> };
type Tool = { execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal, onUpdate?: unknown, ctx?: { cwd: string }) => Promise<Result> };

const stubs: CdpStub[] = [];
const tools: InstanceType<typeof BrowserTool>[] = [];

beforeEach(() => launch.mockReset());

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(tools.splice(0).map((tool) => tool.close()));
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
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

async function setup(handle: CdpHandler, mode: PermissionMode = 'workspace') {
  const cdp = await startCdpStub({ handle });
  stubs.push(cdp);
  vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
  return { cdp, ...register(mode) };
}

function register(mode: PermissionMode = 'workspace') {
  const browser = new BrowserTool(fakePermission(mode));
  tools.push(browser);
  let tool: Tool | undefined;
  const handlers = new Map<string, () => Promise<void>>();
  const pi = { registerTool: (definition: Tool) => (tool = definition), on: (event: string, fn: () => Promise<void>) => handlers.set(event, fn) } as unknown as ExtensionAPI;
  registerBrowserTool(pi, browser);
  const cwd = tmp('octocode-browser-actions-');
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
    // Full mode: an upload is browser control, which the other modes ask about (tests/browser-reach covers that).
    const { cdp, cwd, run, text } = await setup(handler(), 'full');
    // Chrome gets the real path (the temp dir itself sits behind a symlink on macOS).
    const real = fs.realpathSync(cwd);
    fs.writeFileSync(path.join(cwd, 'a.txt'), 'a');
    await expect(run({ action: 'upload', ref: 1 })).rejects.toThrow(/upload needs files/);
    await expect(run({ action: 'upload', ref: 1, files: ['missing.txt'] })).rejects.toThrow(`No such file: ${path.join(real, 'missing.txt')}`);

    expect(await text({ action: 'upload', ref: 1, files: ['a.txt'] })).toBe('Attached to [1]: a.txt');
    expect(cdp.commands.find((command) => command.method === 'DOM.setFileInputFiles')?.params).toEqual({ files: [path.join(real, 'a.txt')], objectId: 'obj-1' });

    expect(await text({ action: 'upload', ref: 2, files: [path.join(cwd, 'a.txt')] })).toBe('Attached to [2]: a.txt');
    const methods = cdp.commands.map((command) => command.method);
    expect(methods).toContain('Page.setInterceptFileChooserDialog');
    expect(cdp.commands.filter((command) => command.method === 'DOM.setFileInputFiles').at(-1)?.params).toEqual({ files: [path.join(real, 'a.txt')], backendNodeId: 77 });
    // Interception is turned back off.
    expect(cdp.commands.filter((command) => command.method === 'Page.setInterceptFileChooserDialog').map((command) => command.params['enabled'])).toEqual([true, false]);
  }, 20_000);
});

describe('browser private-address guard', () => {
  it('checks every page load the page starts, not only the typed URL, and fails a private one', async () => {
    const PRIVATE = 'http://169.254.169.254/latest/meta-data';
    let answer: (method: string) => void = () => undefined;
    const navigate: CdpHandler = (method, params, page) => {
      if (method === 'Page.navigate') {
        // The server redirects `/to-private` to the cloud metadata address: Chrome pauses that hop too.
        const url = String(params['url']).endsWith('/to-private') ? PRIVATE : String(params['url']);
        page.emit('Fetch.requestPaused', { requestId: 'r', request: { url }, resourceType: 'Document', frameId: 'f' });
        return new Promise((resolve) => {
          answer = (reply) => {
            if (reply === 'Fetch.failRequest') return resolve({ frameId: 'f', loaderId: 'l', errorText: 'net::ERR_BLOCKED_BY_CLIENT' });
            setTimeout(() => page.emit('Page.loadEventFired'), 5);
            resolve({ frameId: 'f', loaderId: 'l' });
          };
        });
      }
      if (method === 'Fetch.failRequest' || method === 'Fetch.continueRequest') answer(method);
      return {};
    };
    // Clicking [2] follows a link to a private address.
    const click = (ref: string, page: Parameters<CdpHandler>[2]) => {
      if (ref === '2') page.emit('Fetch.requestPaused', { requestId: 'c', request: { url: 'http://10.0.0.1/admin' }, resourceType: 'Document', frameId: 'f' });
      return { result: { value: true } };
    };
    const { cdp, run, text } = await setup(handler(click, navigate));
    expect(await text({ action: 'navigate', url: 'https://93.184.216.34/' })).toContain('# Stub');
    expect(cdp.commands.find((command) => command.method === 'Fetch.enable')?.params).toEqual({ patterns: [{ urlPattern: '*', requestStage: 'Request' }] });
    expect(cdp.commands.find((command) => command.method === 'Fetch.continueRequest')?.params).toEqual({ requestId: 'r' });

    await expect(run({ action: 'navigate', url: 'https://93.184.216.34/to-private' })).rejects.toThrow(`Navigation failed: net::ERR_BLOCKED_BY_CLIENT (blocked ${PRIVATE}: Blocked private or local address: 169.254.169.254.`);
    expect(cdp.commands.find((command) => command.method === 'Fetch.failRequest')?.params).toEqual({ requestId: 'r', errorReason: 'BlockedByClient' });
    expect(await text({ action: 'console' })).toContain(`[blocked] ${PRIVATE}: Blocked private or local address`);

    // A link the agent clicks is checked the same way, and the result says what was refused.
    expect(await text({ action: 'click', ref: 2 })).toMatch(/^Blocked a page load: http:\/\/10\.0\.0\.1\/admin: Blocked private or local address: 10\.0\.0\.1\./);
    expect(cdp.commands.filter((command) => command.method === 'Fetch.failRequest').at(-1)?.params).toEqual({ requestId: 'c', errorReason: 'BlockedByClient' });
  }, 20_000);
});

describe('browser navigate', () => {
  it('says the page may still be loading when waiting for it to settle failed', async () => {
    const base = handler();
    const failing: CdpHandler = (method, params, page) => {
      if (method === 'Page.navigate') {
        setTimeout(() => page.emit('Page.loadEventFired'), 5);
        return { frameId: 'f', loaderId: 'l' };
      }
      // The page is torn down under the settle wait (another navigation).
      if (method === 'Runtime.evaluate' && String(params['expression']).includes('PerformanceObserver')) return { exceptionDetails: { text: 'Execution context was destroyed' } };
      return base(method, params, page);
    };
    const { text } = await setup(failing);
    const result = await text({ action: 'navigate', url: 'https://93.184.216.34/' });
    expect(result).toContain('# Stub');
    expect(result).toContain('the page may still be loading');
    expect(result).not.toContain('still changing');
  }, 20_000);
});

describe('browser download', () => {
  it('turns downloads on in the session\'s private folder, clicks, waits for the file and lists it with its path', async () => {
    const dir = path.join(sessionOutputDir('output'), 'downloads');
    const file = path.join(dir, 'report.csv');
    const { cdp, cwd, text } = await setup(
      handler((_ref, page) => {
        setTimeout(() => page.emit('Browser.downloadWillBegin', { guid: 'g1', suggestedFilename: 'report.csv', url: 'https://example.com/r' }), 5);
        setTimeout(() => page.emit('Browser.downloadProgress', { guid: 'g1', state: 'inProgress', receivedBytes: 4, totalBytes: 12 }), 10);
        setTimeout(() => (fs.writeFileSync(file, 'a,b\n'), page.emit('Browser.downloadProgress', { guid: 'g1', state: 'completed', receivedBytes: 12, totalBytes: 12, filePath: file })), 150);
        return { result: { value: true } };
      }),
    );
    expect(await text({ action: 'download' })).toBe(`No downloads yet (saving to ${dir}). Pass ref to click a download link.`);
    expect(cdp.commands.find((command) => command.method === 'Browser.setDownloadBehavior')?.params).toEqual({ behavior: 'allow', downloadPath: dir, eventsEnabled: true });
    expect(fs.existsSync(dir)).toBe(true);
    // Nothing lands in the workspace.
    expect(fs.readdirSync(cwd)).toEqual([]);
    expect(await text({ action: 'download', ref: 2 })).toBe(`Downloads in ${dir}:\n- report.csv · 12 bytes · saved · ${file}`);
    // Turned on once per session.
    expect(cdp.commands.filter((command) => command.method === 'Browser.setDownloadBehavior')).toHaveLength(1);
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
    const home = tmp('octocode-browser-actions-');
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
    // Chrome writes DevToolsActivePort only for port 0, so the launch records it (port, then browser path) for the next run.
    expect(fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8')).toBe(`${chrome.port}\n${chrome.browserPath}\n`);

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
    vi.stubEnv('OCTOCODE_HOME', tmp('octocode-browser-actions-'));
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { text } = register();
    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    await text({ action: 'info' });
    await text({ action: 'close' });
    expect(kill).toHaveBeenCalledTimes(1);
  }, 20_000);

  it('reuses the profile Chrome from its DevToolsActivePort, keeping its tab at shutdown', async () => {
    const home = tmp('octocode-browser-actions-');
    const running = await startCdpStub({ handle: handler() });
    stubs.push(running);
    const profile = path.join(home, 'pi-browser-profile');
    fs.mkdirSync(profile, { recursive: true });
    fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${running.port}\n${running.browserPath}\n`);
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

  it('drops a DevToolsActivePort whose port now answers as another browser, and launches on the profile', async () => {
    const home = tmp('octocode-browser-actions-');
    const other = await startCdpStub({ handle: handler() });
    const chrome = await startCdpStub({ handle: handler() });
    stubs.push(other, chrome);
    launch.mockResolvedValue({ port: chrome.port, kill: vi.fn(async () => undefined), process: { unref: vi.fn() } });
    const profile = path.join(home, 'pi-browser-profile');
    fs.mkdirSync(profile, { recursive: true });
    // Our Chrome quit; the port it recorded now belongs to some other browser.
    fs.writeFileSync(path.join(profile, 'DevToolsActivePort'), `${other.port}\n/devtools/browser/gone\n`);
    vi.stubEnv('OCTOCODE_HOME', home);
    vi.stubEnv('OCTOCODE_BROWSER_VISIBLE', '1');
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(await deadPort()));
    const { text } = register();
    await text({ action: 'snapshot' });
    expect(launch).toHaveBeenCalledTimes(1);
    expect(other.commands).toEqual([]);
    expect(fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8')).toBe(`${chrome.port}\n${chrome.browserPath}\n`);
  }, 20_000);

  it('refuses a profile another Chrome holds without DevTools', async () => {
    const home = tmp('octocode-browser-actions-');
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
