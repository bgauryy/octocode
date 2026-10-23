import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, vi } from 'vitest';
import { startCdpStub, type CdpHandler, type CdpStub } from './fixtures/cdp-stub.js';
import { toolContext } from './doubles.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { BrowserTool, registerBrowserTool } from '../src/browser/tool.js';
import type { AuditEntry } from '../src/permission/audit.js';
import type { PermissionMode } from '../src/permission/mode.js';
import type { HostLookup } from '../src/web/guard.js';

/**
 * The `browser` tool's network policy over a stub Chrome (`fixtures/cdp-stub.ts`), with what the permission audits. A test
 * file using it mocks `chrome-launcher` itself: vitest hoists that mock above this import.
 */
export const PRIVATE_HOSTS = ['http://169.254.169.254/latest/meta-data', 'http://10.0.0.1/admin', 'http://[::1]:8080/', 'http://127.0.0.1:9/secret', 'http://192.168.1.1/'];

/** A page whose snapshot and load state answer like a loaded page, plus whatever `extra` does. */
export function pageHandler(extra: CdpHandler = () => undefined): CdpHandler {
  return (method, params, page) => {
    const custom = extra(method, params, page);
    if (custom !== undefined) return custom;
    if (method === 'Runtime.evaluate') {
      const expression = String(params['expression']);
      if (expression.includes('items.push')) return { result: { value: { title: 'Stub', url: 'https://example.com/', text: 'page', items: [] } } };
      if (expression.includes('document.readyState')) return { result: { value: 'complete' } };
      if (expression.includes('location.href')) return { result: { value: 'https://example.com/ | Stub' } };
      return { result: { value: 1 } };
    }
    if (method === 'Page.navigate') return { frameId: 'f' };
    return {};
  };
}

export interface Setup {
  cdp: CdpStub;
  browser: InstanceType<typeof BrowserTool>;
  entries: Array<{ customType: string; data: unknown }>;
  audit: () => AuditEntry[];
  text: (params: Record<string, unknown>) => Promise<string>;
  run: (params: Record<string, unknown>) => Promise<{ content: Array<{ type: string; text?: string }> }>;
  /** The page the session drives now (the stub's end of its WebSocket). */
  page: () => NonNullable<ReturnType<CdpStub['pages']['get']>>;
}

/** Whether `value` is a permission audit entry: it carries the fields every entry has. */
function isAuditEntry(value: unknown): value is AuditEntry {
  if (typeof value !== 'object' || value === null) return false;
  return 'action' in value && typeof value.action === 'string' && 'allowed' in value && typeof value.allowed === 'boolean' && 'what' in value && Array.isArray(value.what) && 'via' in value;
}

/** Registers the cleanup of every stub, tool and folder a test starts; `setup` drives the tool over a stub Chrome. */
export function useNetworkHarness() {
  const stubs: CdpStub[] = [];
  const tools: BrowserTool[] = [];
  const closers: Array<() => unknown> = [];

  afterEach(async () => {
    vi.unstubAllEnvs();
    await Promise.all(tools.splice(0).map((browser) => browser.close()));
    await Promise.all(stubs.splice(0).map((cdp) => cdp.close()));
    while (closers.length > 0) await closers.pop()!();
  });

  /** A stub Chrome answering like a loaded page (`pageHandler`), closed after the test. */
  async function stub(options: Parameters<typeof startCdpStub>[0] = {}): Promise<CdpStub> {
    const cdp = await startCdpStub({ handle: pageHandler(), ...options });
    stubs.push(cdp);
    return cdp;
  }

  /** A browser tool whose permission audits into a fake Pi, closed after the test. */
  function tool(mode: PermissionMode, options: { lookup?: HostLookup; flushMs?: number } = {}) {
    const fake = fakePi();
    const browser = new BrowserTool(fakePermission(mode, { pi: fake.pi }), options);
    tools.push(browser);
    return { browser, entries: fake.entries, audit: () => fake.entries.map(({ customType, data }) => {
      if (!isAuditEntry(data)) throw new Error(`${customType} entry is not a permission audit entry`);
      return data;
    }) };
  }

  async function setup(options: { mode?: PermissionMode; lookup?: HostLookup; flushMs?: number; handle?: CdpHandler; targets?: Parameters<typeof startCdpStub>[0] extends infer O ? (O extends { targets?: infer T } ? T : never) : never } = {}): Promise<Setup> {
    const cdp = await stub({ handle: pageHandler(options.handle), ...(options.targets ? { targets: options.targets } : {}) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { browser, entries, audit } = tool(options.mode ?? 'full', { ...(options.lookup ? { lookup: options.lookup } : {}), ...(options.flushMs ? { flushMs: options.flushMs } : {}) });
    const host = fakePi();
    registerBrowserTool(host.pi, browser);
    const registered = host.tools.get('browser');
    const ctx = toolContext(fakeCtx({ cwd: '/private/tmp' }));
    const run: Setup['run'] = (params) => registered.execute('call-1', params, undefined, undefined, ctx);
    return {
      cdp,
      browser,
      entries,
      audit,
      run,
      text: async (params) => (await run(params)).content.map((part) => part.text ?? '').join('\n'),
      page: () => [...cdp.pages.values()].at(-1)!,
    };
  }

  /** A home folder for the test, removed after it. */
  function fakeHome(): string {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-net-'));
    closers.push(() => fs.rmSync(dir, { recursive: true, force: true }));
    return dir;
  }

  return { stub, tool, setup, fakeHome };
}

/** Opens `url` as the agent does and records its approval for the tab the session drives now. */
export const openFor = async (browser: BrowserTool, url: string): Promise<void> => (await browser.open(url))((await browser.session()).targetId);

/** A tab's frame navigated, as Chrome reports it; the main frame unless `frame` names a parent. */
export const navigates = (page: { emit: (method: string, params?: Record<string, unknown>) => void }, url: string, frame: Record<string, unknown> = {}) => page.emit('Page.frameNavigated', { frame: { id: 'main', url, ...frame } });

export const paused = (requestId: string, url: string, resourceType: string, extra: Record<string, unknown> = {}) => ({ requestId, request: { url }, resourceType, frameId: 'f', ...extra });
export const answered = (cdp: CdpStub, method: 'Fetch.failRequest' | 'Fetch.continueRequest'): string[] => cdp.commands.filter((command) => command.method === method).map((command) => String(command.params['requestId']));
