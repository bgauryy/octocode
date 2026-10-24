import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { initTheme } from '@earendil-works/pi-coding-agent';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NO_REPLY } from './fixtures/cdp-stub.js';
import { theme } from './fake-pi.js';
import { pageHandler, useBrowserHarness, type Result } from './browser-harness.js';

const launch = vi.hoisted(() => vi.fn());
vi.mock('chrome-launcher', () => ({ launch }));
vi.mock('node:dns/promises', () => ({ lookup: async () => [{ address: '93.184.216.34', family: 4 }] })); // public: no network

const { browserSummary } = await import('../src/browser/render.js');
const { openBrowser } = await import('../src/browser/session.js');
const { stub, register } = useBrowserHarness();

beforeEach(() => {
  launch.mockReset();
});

/** A stub Chrome on the shared port, the tool, and (unless `navigate` is false) the stub page loaded. */
async function openPage(navigate = true) {
  const cdp = await stub();
  vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
  const tool = register();
  if (navigate) await tool.text({ action: 'navigate', url: 'https://example.com/' });
  return { cdp, ...tool };
}

describe('browser tool actions on its own tab', () => {
  it('reports no browser until an action starts one, then opens its own tab on the shared Chrome', async () => {
    const { cdp, text } = await openPage(false);
    expect(await text({ action: 'info' })).toBe('No browser is open. navigate or attach starts one.');
    await text({ action: 'snapshot' }); // info reports a browser; it never starts one
    expect(await text({ action: 'info' })).toBe(`shared Chrome, own tab · DevTools port ${cdp.port} · https://example.com/ | Stub page`);
    expect(cdp.requests).toContain('PUT /json/new?about:blank');
    expect(cdp.commands.slice(0, 2).map((command) => command.method).sort()).toEqual(['Page.enable', 'Runtime.enable']);
  });

  it('navigates to a snapshot of elements and text', async () => {
    const { text } = await openPage(false);
    const snapshot = await text({ action: 'navigate', url: 'https://example.com/' });
    expect(snapshot).toContain('# Stub page\nhttps://example.com/\n\n## Elements\n[1] button "Go"');
    expect(snapshot).toContain('## Text\nHello from the stub');
    expect(await text({ action: 'snapshot' })).toContain('[2] input:text "Name"');
  });

  it('clicks an element by ref, returns only the changes on the same page, and names an element that covers the target', async () => {
    const { cdp, text, run } = await openPage();
    expect(await text({ action: 'click', ref: 1 })).toBe('# Stub page\nhttps://example.com/ (same page)\n\n## Elements\nUnchanged (2).\n\n## New text\nNo new text.');
    expect(cdp.commands.filter((command) => command.method === 'Input.dispatchMouseEvent').map((command) => command.params['type'])).toEqual(['mouseMoved', 'mousePressed', 'mouseReleased']);
    expect(cdp.commands.find((command) => command.method === 'Input.dispatchMouseEvent')?.params).toMatchObject({ x: 10, y: 20 });
    expect(await text({ action: 'click', ref: 2 })).toMatch(/^Note: div#overlay "Cookie banner" covers \[2\], so the click landed on it\./);
    await expect(run({ action: 'click', ref: 9 })).rejects.toThrow(/No element \[9\]/);
    await expect(run({ action: 'click' })).rejects.toThrow(/needs ref/);
  });

  it('types into a field and submits with Enter, and presses keys by their codes', async () => {
    const { cdp, text, run } = await openPage();
    await text({ action: 'type', ref: 2, text: 'Ada', submit: true });
    expect(cdp.commands.find((command) => command.method === 'Input.insertText')?.params).toEqual({ text: 'Ada' });
    expect(cdp.commands.filter((command) => command.method === 'Input.dispatchKeyEvent').map((command) => command.params)).toEqual([
      { type: 'keyDown', key: 'Enter', code: 'Enter', modifiers: 0, windowsVirtualKeyCode: 13, text: '\r' },
      { type: 'keyUp', key: 'Enter', code: 'Enter', modifiers: 0, windowsVirtualKeyCode: 13 },
    ]);
    await expect(run({ action: 'type', ref: 7, text: 'x' })).rejects.toThrow(/No element \[7\]/);
    await expect(run({ action: 'type', ref: 2 })).rejects.toThrow(/type needs text/);
    await expect(run({ action: 'press' })).rejects.toThrow(/press needs key/);
    await text({ action: 'press', key: '5' });
    await text({ action: 'press', key: 'a' });
    const codes = cdp.commands.filter((command) => command.method === 'Input.dispatchKeyEvent' && command.params['type'] === 'keyDown').map((command) => command.params['code']);
    expect(codes.slice(-2)).toEqual(['Digit5', 'KeyA']);
  });

  it('evaluates expressions to text or JSON and reports page exceptions as visible escapes', async () => {
    const { text, run } = await openPage();
    expect(await text({ action: 'evaluate', expression: 'document.title' })).toBe('Stub page');
    expect(JSON.parse(await text({ action: 'evaluate', expression: '({ answer: 42 })' }))).toEqual({ answer: 42 });
    expect(await text({ action: 'evaluate', expression: 'undefined' })).toBe('undefined');
    await expect(run({ action: 'evaluate', expression: 'boom()' })).rejects.toThrow('ReferenceError: boom is not defined');
    // A page's exception text reaches the model as visible escapes, not terminal sequences.
    await expect(run({ action: 'evaluate', expression: 'bad()' })).rejects.toThrow('Error: \\u{1b}[2Jwiped\\u{202e}');
    await expect(run({ action: 'evaluate' })).rejects.toThrow(/needs expression/);
  });

  it('returns a screenshot as an image with a caption', async () => {
    const { run } = await openPage();
    const shot = await run({ action: 'screenshot' });
    expect(shot.content).toMatchObject([{ type: 'image', data: 'iVBORw0KGgo=', mimeType: 'image/jpeg' }, { type: 'text' }]);
  });

  it('lists console messages and exceptions in order', async () => {
    const { cdp, text } = await openPage();
    expect(await text({ action: 'console' })).toBe('(no console messages)');
    const page = cdp.pages.get('tab-1')!;
    page.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'hello' }, { description: 'Object' }, {}] });
    page.emit('Runtime.exceptionThrown', { exceptionDetails: { exception: { description: 'Error: bad' } } });
    page.emit('Runtime.exceptionThrown', { exceptionDetails: { text: 'Uncaught' } });
    page.emit('Runtime.consoleAPICalled', { type: 'warn' });
    await vi.waitFor(async () => expect(await text({ action: 'console' })).toBe('[log] hello Object \n[exception] Error: bad\n[exception] Uncaught\n[warn] '));
  });

  it('closes its own tab', async () => {
    const { cdp, text } = await openPage();
    expect(await text({ action: 'close' })).toBe('Browser closed.');
    expect(cdp.requests).toContain('GET /json/close/tab-1');
  });
});

describe('browser tool over a shared Chrome', () => {
  it('says how much console output it no longer holds: lines pushed out, a cut line, and lines of an earlier tab', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { text, browser } = register();
    await text({ action: 'snapshot' });
    const page = cdp.pages.get('tab-1')!;
    for (let i = 1; i <= 205; i++) page.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: `line ${i}` }] });
    page.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'x'.repeat(2_500) }] });
    await vi.waitFor(async () => expect(await text({ action: 'console' })).toContain('x'.repeat(100)));
    const shown = await text({ action: 'console' });
    // 206 lines in a buffer of 200: the 6 oldest are gone from the view, the first kept is line 7; the long line says what was cut.
    const [first, where] = shown.split('\n');
    expect(first).toBe('[6 earlier console lines dropped from this view: only the last 200 are kept]');
    const file = /every console line is in (\S+); search it/.exec(where!)?.[1];
    expect(file).toBeDefined();
    expect(shown).toContain('[log] line 7\n');
    expect(shown).not.toContain('[log] line 6\n');
    expect(shown).toMatch(/x… \[\d+ more characters cut; the whole line is in \S+\]$/);
    // The named file holds every line, whole: the dropped ones and the uncut long one.
    const saved = fs.readFileSync(path.resolve(file!.replace(/^~/, os.homedir())), 'utf8').split('\n');
    expect(saved.slice(0, 6)).toEqual(['[log] line 1', '[log] line 2', '[log] line 3', '[log] line 4', '[log] line 5', '[log] line 6']);
    expect(saved).toContain(`[log] ${'x'.repeat(2_500)}`);
    // A tab change discards the earlier tab's lines and counts them (see the tab-policy test); the result names the count and the log.
    const session = await browser.session();
    session.console = [];
    session.consoleLost = { dropped: 0, cleared: 206 };
    expect(await text({ action: 'console' })).toMatch(/^\[206 lines from earlier tabs left this view when the tab changed\]\n\[every console line is in \S+; search it or read it by line range\]\n\(no console messages\)$/);
  }, 20_000);

  it('reports navigation errors and does not wait for a load event on a same-document navigation', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run, text } = register();
    await expect(run({ action: 'navigate' })).rejects.toThrow(/needs url/);
    await expect(run({ action: 'navigate', url: 'https://unreachable.invalid/' })).rejects.toThrow('Navigation failed: net::ERR_NAME_NOT_RESOLVED');
    const started = Date.now();
    expect(await text({ action: 'navigate', url: 'https://example.com/#section' })).toContain('# Stub page');
    expect(Date.now() - started).toBeLessThan(5_000);
  }, 20_000);

  it('stops a navigation when aborted, leaving no pending load wait behind', async () => {
    const cdp = await stub({ handle: (method, params, page) => (method === 'Page.navigate' ? NO_REPLY : pageHandler(method, params, page)) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { tool } = register();
    const controller = new AbortController();
    const pending = (tool.execute as (id: string, params: Record<string, unknown>, signal: AbortSignal) => Promise<Result>)('id', { action: 'navigate', url: 'https://slow.example/' }, controller.signal);
    await vi.waitFor(() => expect(cdp.commands.some((command) => command.method === 'Page.navigate')).toBe(true));
    controller.abort(new Error('user stopped'));
    await expect(pending).rejects.toThrow('user stopped');
  }, 20_000);

  it('reopens the session after the page connection drops, rejecting what was pending', async () => {
    const cdp = await stub();
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { browser, text } = register();
    const first = await browser.session();
    const pending = first.page.send('Slow.never');
    cdp.pages.get('tab-1')!.drop();
    await expect(pending).rejects.toThrow('Browser connection closed');
    await vi.waitFor(() => expect(first.page.open).toBe(false));
    await expect(first.page.send('Page.enable')).rejects.toThrow('Browser connection closed');
    expect(await text({ action: 'snapshot' })).toContain('# Stub page');
    expect(cdp.requests).toContain('GET /json/close/tab-1');
    expect(cdp.pages.has('tab-2')).toBe(true);
  }, 20_000);

  it('throws when the page cannot be read and closes on session shutdown', async () => {
    const cdp = await stub({ handle: (method, params, page) => (method === 'Runtime.evaluate' && String(params['expression']).includes('items.push') ? { result: {} } : pageHandler(method, params, page)) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run, handlers } = register();
    await expect(run({ action: 'snapshot' })).rejects.toThrow(/Could not read the page/);
    await handlers.get('session_shutdown')!();
    expect(cdp.requests).toContain('GET /json/close/tab-1');
  }, 20_000);

  it('renders an evaluate result JSON cannot carry (NaN, Infinity, BigInt, a function) instead of printing undefined', async () => {
    const results: Record<string, unknown> = {
      nan: { type: 'number', unserializableValue: 'NaN', description: 'NaN' },
      inf: { type: 'number', unserializableValue: '-Infinity', description: '-Infinity' },
      big: { type: 'bigint', unserializableValue: '10n', description: '10n' },
      fn: { type: 'function', className: 'Function', description: '() => 1', objectId: 'o' },
      nothing: { type: 'undefined' },
    };
    const cdp = await stub({ handle: (method, params, page) => (method === 'Runtime.evaluate' && String(params['expression']) in results ? { result: results[String(params['expression'])] } : pageHandler(method, params, page)) });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { text } = register();
    expect(await text({ action: 'evaluate', expression: 'nan' })).toBe('NaN');
    expect(await text({ action: 'evaluate', expression: 'inf' })).toBe('-Infinity');
    expect(await text({ action: 'evaluate', expression: 'big' })).toBe('10n');
    expect(await text({ action: 'evaluate', expression: 'fn' })).toBe('() => 1');
    expect(await text({ action: 'evaluate', expression: 'nothing' })).toBe('undefined');
  }, 20_000);

  it('shows terminal controls in page output as visible escapes and saves a large evaluate result to a file', async () => {
    const home = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-browser-home-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    const big = Array.from({ length: 5000 }, (_, n) => `row ${n + 1} ${'q'.repeat(40)}`).join('\n');
    const cdp = await stub({
      handle: (method, params, page) => {
        const expression = String(params['expression'] ?? '');
        if (method === 'Runtime.evaluate' && expression === 'hostile') return { result: { value: 'a\u001b[31mred\u001b[0m \u202eevil' } };
        if (method === 'Runtime.evaluate' && expression === 'big') return { result: { value: big } };
        return pageHandler(method, params, page);
      },
    });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { run } = register();
    const textOf = async (args: Record<string, unknown>) => ((await run(args)).content as Array<{ text: string }>)[0]!.text;
    expect(await textOf({ action: 'evaluate', expression: 'hostile' })).toBe('a\\u{1b}[31mred\\u{1b}[0m \\u{202e}evil');
    const saved = await textOf({ action: 'evaluate', expression: 'big' });
    expect(saved).toContain('row 1 ');
    expect(saved).toContain('row 5000 ');
    const file = /Full output: (\S+?\.txt)/.exec(saved)?.[1];
    expect(file?.startsWith(path.join(home, 'agent', 'pi', 'sessions'))).toBe(true);
    expect(fs.readFileSync(file!, 'utf8')).toBe(big);
  }, 20_000);

  it('shows page text, console lines, the console log file and a timed-out wait\'s page as visible escapes: nothing raw, nothing dropped', async () => {
    vi.stubEnv('OCTOCODE_HOME', fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-browser-home-')));
    const hostile = { title: 'T\u001b]0;pwned\u0007', url: 'https://example.com/', text: 'zero\u200bwidth \u202eevil\rlone\r\ncrlf', items: ['[1] button "Go\u009b"'] };
    const cdp = await stub({
      handle: (method, params, page) => (method === 'Runtime.evaluate' && String(params['expression']).includes('items.push') ? { result: { value: hostile } } : pageHandler(method, params, page)),
    });
    vi.stubEnv('OCTOCODE_CHROME_PORT', String(cdp.port));
    const { text, run, browser } = register();
    // Page text (title, element names, body text): CRLF stays, a lone CR is `\r`.
    const page = await text({ action: 'snapshot' });
    expect(page).toContain('# T\\u{1b}]0;pwned\\u{7}');
    expect(page).toContain('[1] button "Go\\u{9b}"');
    expect(page).toContain('zero\\u{200b}width \\u{202e}evil\\rlone\r\ncrlf');
    expect(page).not.toMatch(/[\u001b\u0007\u009b\u200b\u202e]|\r(?!\n)/u);
    // Console lines in the result and in the log file.
    const live = cdp.pages.get('tab-1')!;
    live.emit('Runtime.consoleAPICalled', { type: 'log', args: [{ value: 'a\u001b[31mred\u001b[0m \u202eevil' }] });
    live.emit('Runtime.exceptionThrown', { exceptionDetails: { exception: { description: 'Error: \u0000nul' } } });
    const expected = '[log] a\\u{1b}[31mred\\u{1b}[0m \\u{202e}evil\n[exception] Error: \\u{0}nul';
    await vi.waitFor(async () => expect(await text({ action: 'console' })).toBe(expected));
    const logFile = (await browser.session()).consoleLog!.file!;
    await vi.waitFor(() => expect(fs.readFileSync(logFile, 'utf8')).toBe(`${expected}\n`));
    // Error text: a wait that times out carries the current page, escaped.
    const failure = await run({ action: 'wait', text: 'never appears', timeout: 1 }).catch((error: Error) => error.message);
    expect(failure).toMatch(/^Timed out after 1s waiting for "never appears" to appear\./);
    expect(failure).toContain('# T\\u{1b}]0;pwned\\u{7}');
    expect(failure).not.toMatch(/[\u001b\u0007\u009b\u200b\u202e]|\r(?!\n)/u);
  }, 20_000);

  it('renders calls and results on one line each', () => {
    initTheme('dark');
    const { tool } = register();
    const line = (args: Record<string, unknown>) => tool.renderCall(args, theme, {}).render(200).join('\n').trim();
    expect(line({ action: 'navigate', url: 'https://example.com' })).toBe('○ Browser(navigate https://example.com)');
    expect(line({ action: 'click', ref: 3 })).toBe('○ Browser(click [3])');
    expect(line({ action: 'type', ref: 2, text: 'hello' })).toBe('○ Browser(type [2] "hello")');
    expect(line({ action: 'evaluate', expression: 'a\nb' })).toBe('○ Browser(evaluate a)');
    expect(line({ action: 'press', key: 'Tab' })).toBe('○ Browser(press Tab)');
    const render = (content: Array<{ type: string; text?: string }>, context: Record<string, unknown> = {}) => tool.renderResult({ content }, { expanded: false }, theme, { isError: false, ...context }).render(200).join('\n');
    const plain = render([{ type: 'image' }, { type: 'text', text: 'one\ntwo\nthree\nfour\nfive' }]);
    expect(plain).toContain('⎿  one');
    expect(plain).toContain('… +1 line');
    expect(render([{ type: 'image' }])).toContain('⎿  Screenshot');
    const snapshot = render([{ type: 'text', text: '# Example\nhttps://example.com\n\n## Elements\n[1] link "More"\n[Full snapshot (every element and all text): /tmp/snap.txt; read it by line range or search it.]' }]);
    expect(snapshot).toContain('⎿  Example — https://example.com');
    expect(snapshot).not.toContain('saved:');
    const snapshotText = '# Example\nhttps://example.com\n\n## Elements\n[1] link "More"\n[Full snapshot (every element and all text): /tmp/snap.txt; read it by line range or search it.]';
    expect(tool.renderResult({ content: [{ type: 'text', text: snapshotText }] }, { expanded: true }, theme, { isError: false, expanded: true }).render(200).join('\n')).toContain('saved: /tmp/snap.txt');
    expect(render([{ type: 'text', text: 'Browser not running\nstart it' }], { isError: true })).toContain('⎿  Error: Browser not running');
    expect(browserSummary('# \nhttps://a.example (same page)\nrest')).toEqual({ summary: '(untitled) — https://a.example (same page)', body: 'rest' });
    expect(browserSummary('Filled 1 of 1 field.\n\n# T\nu')).toMatchObject({ summary: 'Filled 1 of 1 field.' });
  });
});

describe('browser attach', () => {
  it('drives the existing page of the browser on a given port', async () => {
    const cdp = await stub({ targets: [{ id: 'worker', type: 'service_worker' }, { id: 'nows', type: 'page', ws: false }, { id: 'page-1', type: 'page' }] });
    const { text, run } = register();
    await expect(run({ action: 'attach' })).rejects.toThrow(/attach needs port/);

    const attached = await text({ action: 'attach', port: cdp.port });
    expect(attached).toContain(`Attached to the browser on DevTools port ${cdp.port}.`);
    expect(attached).toContain('# Stub page');
    expect(cdp.pages.has('page-1')).toBe(true);
    expect(cdp.requests.some((request) => request.startsWith('PUT /json/new'))).toBe(false);
    expect(await text({ action: 'info' })).toMatch(/^attached window · DevTools port \d+ · /);

    expect(await text({ action: 'attach', port: cdp.port })).toContain('Attached');
    await text({ action: 'close' });
    // Someone else's page: never closed by us.
    expect(cdp.requests.some((request) => request.startsWith('GET /json/close'))).toBe(false);
  }, 20_000);

  it('fails when nothing listens on the port', async () => {
    const cdp = await stub();
    const port = cdp.port;
    await cdp.close();
    await expect(openBrowser({}, { port, reuseTab: true })).rejects.toThrow(`No browser is listening on DevTools port ${port}.`);
  });

  it('opens a tab when the browser lists no page', async () => {
    const cdp = await stub({ targets: [] });
    const session = await openBrowser({}, { port: cdp.port, reuseTab: true });
    expect(session.kind).toBe('attached');
    expect(cdp.pages.has('tab-1')).toBe(true);
    session.page.close();
  });
});
