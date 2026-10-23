import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { startCdpStub, type CdpStub } from './fixtures/cdp-stub.js';
import { fakeCtx, fakePermission } from './fake-pi.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));
vi.mock('node:dns/promises', () => ({ lookup: async () => [{ address: '93.184.216.34', family: 4 }] })); // public: no network

const { BrowserTool, registerBrowserTool } = await import('../src/browser/tool.js');
const { ORIGINS_MAX, noteOrigin } = await import('../src/browser/session-state.js');

type Result = { content: Array<{ type: string; text?: string; data?: string }> };
type Tool = { execute: (id: string, params: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: undefined, ctx: ExtensionContext) => Promise<Result> };

const stubs: CdpStub[] = [];
const tools: InstanceType<typeof BrowserTool>[] = [];

beforeEach(() => launch.mockReset());

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(tools.splice(0).map((tool) => tool.close()));
  await Promise.all(stubs.splice(0).map((stub) => stub.close()));
});

async function stub(options: Parameters<typeof startCdpStub>[0] = {}): Promise<CdpStub> {
  const started = await startCdpStub(options);
  stubs.push(started);
  return started;
}

function register() {
  const browser = new BrowserTool(fakePermission());
  tools.push(browser);
  let tool: Tool | undefined;
  const pi = { registerTool: (definition: Tool) => (tool = definition), on: () => undefined } as unknown as ExtensionAPI;
  registerBrowserTool(pi, browser);
  const run = (params: Record<string, unknown>, model?: unknown) => tool!.execute('id', params, undefined, undefined, fakeCtx({ cwd: process.cwd(), hasUI: false, ...(model ? { model } : {}) }));
  const text = async (params: Record<string, unknown>) => (await run(params)).content.map((part) => part.text ?? `[${part.type}]`).join('\n');
  return { run, text };
}

describe('browser screenshots', () => {
  const BIG = 'A'.repeat(700_000);
  /** Captures answer by what was asked: `sizes` maps a clip scale (or 'full') to the base64 the page returns. */
  async function shots(sizes: Record<string, string>, origin = { x: 0, y: 0 }) {
    const cdp = await stub({
      handle: (method, params) => {
        if (method === 'Page.getLayoutMetrics') return { cssVisualViewport: { clientWidth: 1280, clientHeight: 800, pageX: origin.x, pageY: origin.y } };
        if (method === 'Page.captureScreenshot') {
          const clip = params['clip'] as { scale: number } | undefined;
          return { data: sizes[clip ? String(clip.scale) : 'full'] ?? 'AAAA' };
        }
        return {};
      },
    });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    return { cdp, ...register() };
  }
  const captures = (cdp: CdpStub) => cdp.commands.filter((command) => command.method === 'Page.captureScreenshot').map((command) => command.params);
  const savedPath = (result: Result) => /Full-size file: (\S+)/.exec(result.content.find((part) => part.type === 'text')?.text ?? '')?.[1];

  it('captures a jpeg of the visible page by default and saves the full file, naming its path', async () => {
    const small = Buffer.from('jpeg-pixels').toString('base64');
    const { cdp, run } = await shots({ full: small });
    const result = await run({ action: 'screenshot' });
    expect(captures(cdp)).toEqual([{ format: 'jpeg', quality: 70, captureBeyondViewport: false }]);
    expect(result.content[0]).toEqual({ type: 'image', data: small, mimeType: 'image/jpeg' });
    expect(fs.readFileSync(savedPath(result)!, 'utf8')).toBe('jpeg-pixels');
    expect(path.extname(savedPath(result)!)).toBe('.jpg');
    expect(result.content[1]!.text).toMatch(/^Screenshot of the visible page \(jpeg, 1 KB\)\. Full-size file: /);
  });

  it('captures a png only when asked', async () => {
    const { cdp, run } = await shots({ full: 'iVBORw0KGgo=' });
    const result = await run({ action: 'screenshot', format: 'png' });
    expect(captures(cdp)).toEqual([{ format: 'png', captureBeyondViewport: false }]);
    expect(result.content[0]).toMatchObject({ type: 'image', mimeType: 'image/png' });
    expect(path.extname(savedPath(result)!)).toBe('.png');
    expect(result.content[1]!.text).toContain('(png, 1 KB)');
  });

  it('retakes an oversized capture smaller, keeps the full file and says so', async () => {
    const { cdp, run } = await shots({ full: BIG, '0.5': 'QUJD' });
    const result = await run({ action: 'screenshot' });
    expect(captures(cdp).map((params) => (params['clip'] as { scale: number } | undefined)?.scale)).toEqual([undefined, 0.5]);
    expect(captures(cdp)[1]).toMatchObject({ clip: { x: 0, y: 0, width: 1280, height: 800, scale: 0.5 } });
    expect(result.content[0]).toEqual({ type: 'image', data: 'QUJD', mimeType: 'image/jpeg' });
    expect(result.content[1]!.text).toMatch(/^Screenshot of the visible page, retaken smaller to fit \(jpeg at 0\.5x scale, 1 KB; limit 439 KB\)\./);
    expect(fs.statSync(savedPath(result)!).size).toBe(Buffer.from(BIG, 'base64').length);
  });

  it('returns only the saved path when no retake fits the budget', async () => {
    const { run } = await shots({ full: BIG, '0.5': BIG, '0.25': BIG });
    const result = await run({ action: 'screenshot' });
    expect(result.content.map((part) => part.type)).toEqual(['text']);
    expect(result.content[0]!.text).toMatch(/^Screenshot too large for the context, not shown \(jpeg, \d+ KB; limit 439 KB\)\./);
    expect(fs.existsSync(savedPath(result)!)).toBe(true);
  });

  it("takes the budget from the model's own byte limit when it is smaller, and says which limit applied", async () => {
    const mid = 'A'.repeat(300_000);
    const model = { provider: 'p', id: 'm', inputLimits: { images: { resize: { maxBytes: 100_000 } } } };
    const { cdp, run } = await shots({ full: mid, '0.5': 'QUJD' });
    const result = await run({ action: 'screenshot' }, model);
    // 300,000 characters fit the default budget but not this model's 100,000: retaken smaller.
    expect(captures(cdp).map((params) => (params['clip'] as { scale: number } | undefined)?.scale)).toEqual([undefined, 0.5]);
    expect(result.content[1]!.text).toMatch(/retaken smaller to fit \(jpeg at 0\.5x scale, 1 KB; limit 73 KB\)/);
    const unlimited = await run({ action: 'screenshot' });
    expect(unlimited.content[0]).toMatchObject({ type: 'image', data: mid });
  });

  it('retakes the visible region after horizontal and vertical scrolling', async () => {
    const { cdp, run } = await shots({ full: BIG, '0.5': 'QUJD' }, { x: 44, y: 1100 });
    await run({ action: 'screenshot' });
    expect(captures(cdp)[1]).toMatchObject({ clip: { x: 44, y: 1100, width: 1280, height: 800, scale: 0.5 } });
  });
});

describe('browser page events are bounded', () => {
  it('clips a long console message and keeps only the most recent downloads', async () => {
    const cdp = await stub({
      handle: (method, _params, page) => {
        if (method !== 'Page.captureScreenshot') return {};
        page.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: `start-${'x'.repeat(10_000)}-end` }] });
        for (let index = 0; index < 150; index++) {
          page.emit('Browser.downloadWillBegin', { guid: `g${index}`, suggestedFilename: `file-${index}.bin` });
          page.emit('Browser.downloadProgress', { guid: `g${index}`, state: 'completed', receivedBytes: 1 });
        }
        return { data: 'AAAA' };
      },
    });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { text } = register();
    await text({ action: 'screenshot' });
    const consoleText = await text({ action: 'console' });
    expect(consoleText.startsWith('[log] start-')).toBe(true);
    expect(consoleText.length).toBeLessThan(2_400);
    expect(consoleText).toMatch(/… \[\d+ more characters cut; the whole line is in \S+\]$/);
    const listing = (await text({ action: 'download' })).split('\n');
    expect(listing.some((line) => line.includes('file-149.bin'))).toBe(true);
    expect(listing.some((line) => line.includes('file-0.bin'))).toBe(false);
    expect(listing.length).toBeLessThanOrEqual(102);
  });
});

/** Just the parts of a session `noteOrigin` reads: the origins noted and the origin each driven tab shows. */
const sessionWith = (tabOrigins: Record<string, string> = {}) => ({ origins: new Set<string>(), tabOrigins: new Map(Object.entries(tabOrigins)) }) as unknown as Parameters<typeof noteOrigin>[0];

describe('the origins a session remembers are bounded', () => {
  it('forgets the origin noted longest ago at the bound, and keeps one that was loaded again', () => {
    const session = sessionWith();
    noteOrigin(session, 'https://first.example/');
    for (let n = 0; n < ORIGINS_MAX - 1; n++) noteOrigin(session, `https://site-${n}.example/`);
    expect(session.origins.size).toBe(ORIGINS_MAX);
    noteOrigin(session, 'https://first.example/again');
    noteOrigin(session, 'https://new.example/');
    expect(session.origins.size).toBe(ORIGINS_MAX);
    expect(session.origins.has('https://first.example')).toBe(true);
    expect(session.origins.has('https://new.example')).toBe(true);
    expect(session.origins.has('https://site-0.example')).toBe(false);
    expect(session.origins.has('https://site-1.example')).toBe(true);
    noteOrigin(session, 'about:blank');
    noteOrigin(session, 'not a url');
    expect(session.origins.size).toBe(ORIGINS_MAX);
  });

  it('never forgets the origin a driven tab shows, however many others a page loads: the oldest unpinned one goes', () => {
    const session = sessionWith({ 'tab-1': 'https://evil.example' });
    noteOrigin(session, 'https://evil.example/');
    for (let n = 0; n < ORIGINS_MAX * 2; n++) noteOrigin(session, `https://a${n}.example/`);
    expect(session.origins.has('https://evil.example')).toBe(true);
    expect(session.origins.size).toBe(ORIGINS_MAX);
    // The pinned origin took one slot; the oldest unpinned ones were forgotten, the newest kept.
    expect(session.origins.has('https://a0.example')).toBe(false);
    expect(session.origins.has(`https://a${ORIGINS_MAX * 2 - 1}.example`)).toBe(true);
  });

  it('keeps every origin when all are pinned, past the bound, and forgets one again once its tab leaves it', () => {
    const tabs = Object.fromEntries(Array.from({ length: ORIGINS_MAX + 5 }, (_, n) => [`tab-${n}`, `https://t${n}.example`]));
    const session = sessionWith(tabs);
    for (let n = 0; n < ORIGINS_MAX + 5; n++) noteOrigin(session, `https://t${n}.example/`);
    expect(session.origins.size).toBe(ORIGINS_MAX + 5);
    session.tabOrigins.delete('tab-0');
    noteOrigin(session, 'https://next.example/');
    expect(session.origins.has('https://t0.example')).toBe(false);
    expect(session.origins.size).toBe(ORIGINS_MAX + 5);
  });
});
