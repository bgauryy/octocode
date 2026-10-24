import fs from 'node:fs';
import os from 'node:os';
import type { ExtensionContext, ExtensionToolContext, ToolDefinition } from '@earendil-works/pi-coding-agent';
import { describe, expect, it } from 'vitest';
import { registerOctocodeMcp } from '../src/mcp/octocode.js';
import { octocodeToolDefinition, sanitizeJson, TEXT_MAX_CHARS, toToolResult, TURN_MAX_CHARS, TurnBudget } from '../src/mcp/tools.js';
import { stub, textOf } from './doubles.js';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { noisyPng, pixelSize, tmp } from './helpers.js';

const tool = { name: 'localFetch', description: ' Read files ', inputSchema: { type: 'object' as const }, annotations: { readOnlyHint: true, title: 'x' } };
const binding = { prefix: 'mcp__octocode__', namespace: { name: 'mcp__octocode' }, call: async () => ({ content: [{ type: 'text' as const, text: 'ok' }] }) };
type RenderContext = Parameters<NonNullable<ToolDefinition['renderCall']>>[2];
const context = (extra: Partial<RenderContext> = {}) => stub<RenderContext>({ lastComponent: undefined, state: {}, args: {}, toolCallId: 'c', invalidate: () => undefined, cwd: '/w', executionStarted: true, argsComplete: true, isPartial: false, expanded: false, showImages: false, isError: false, ...extra });
const fileOf = (preview: string) => fs.readFileSync(/Full output: (\S+?); search/.exec(preview)![1].replace(/^~/, os.homedir()), 'utf8');
/** A page like the server's: rows, then the `next` continuation at the very end. */
const serverPage = (bytes: number, mark: string) => `${'- row: some repository text\n'.repeat(Math.ceil(bytes / 28))}next:\n  continueChars: { charOffset: ${mark} }\n`;
const shownText = (result: Awaited<ReturnType<typeof toToolResult>>) => textOf(result.content[0]);

describe('Octocode tool results and rendering', () => {
  it('gives a page over 20 KB to the model whole: no cut, no marker, non-ASCII intact', async () => {
    const page = `${'- row: héllo wörld — 日本語\n'.repeat(1_100)}  next:\n    continueChars: { charOffset: 20000 }\n${'- last row ✓\n'.repeat(300)}`;
    expect(Buffer.byteLength(page)).toBeGreaterThan(40 * 1024);
    expect(page.length).toBeLessThan(TEXT_MAX_CHARS);
    const result = await toToolResult('localFetch', { content: [{ type: 'text', text: page }, { type: 'image', data: 'aW1n', mimeType: 'image/png' }] }, false);
    expect(result.content).toEqual([{ type: 'text', text: page }, { type: 'image', data: 'aW1n', mimeType: 'image/png' }]);
    expect(result).not.toHaveProperty('structuredContent');
    expect(result).not.toHaveProperty('isError');
  });

  it('spills text over the budget to a file and replaces an oversized image by a note naming its file', async () => {
    const huge = `${'- row: some repository text\n'.repeat(12_000)}tail line`;
    expect(Buffer.byteLength(huge)).toBeGreaterThan(300_000);
    const bigImage = 'A'.repeat(700_000);
    const result = await toToolResult('localSearch', { content: [{ type: 'text', text: huge }, { type: 'image', data: bigImage, mimeType: 'image/png' }, { type: 'image', data: 'aW1n', mimeType: 'image/png' }] }, false);
    const [text, dropped, kept] = result.content as Array<{ type: string; text?: string; data?: string }>;
    expect(text.text!.length).toBeLessThanOrEqual(TEXT_MAX_CHARS);
    expect(text.text).toContain('tail line');
    const textFile = /Full output: (\S+?); search/.exec(text.text!)?.[1];
    expect(fs.readFileSync(textFile!.replace(/^~/, os.homedir()), 'utf8')).toBe(huge);
    expect(textFile).toMatch(/mcp_localSearch/);
    expect(dropped).toMatchObject({ type: 'text' });
    const imageFile = /Saved to (\S+?)\.\]/.exec(dropped.text!)?.[1];
    expect(fs.statSync(imageFile!).size).toBe(Buffer.from(bigImage, 'base64').length);
    expect(kept).toEqual({ type: 'image', data: 'aW1n', mimeType: 'image/png' });
  });

  it('turns other blocks into text, an empty error into a message, and keeps structuredContent only when declared', async () => {
    const linked = await toToolResult('t', { content: [{ type: 'resource_link', uri: 'file:///a', name: 'a' }] }, false);
    expect(linked.content).toHaveLength(1);
    expect(linked.content[0]).toMatchObject({ type: 'text' });
    expect(await toToolResult('t', { content: [], isError: true }, false)).toMatchObject({ isError: true, content: [{ type: 'text', text: 't failed without a message.' }] });
    const structured = { content: [{ type: 'text' as const, text: '{}' }], structuredContent: { a: 1 } };
    expect((await toToolResult('t', structured, true)).structuredContent).toEqual({ a: 1 });
    expect(await toToolResult('t', structured, false)).not.toHaveProperty('structuredContent');
  });
});

describe('Octocode tool definitions and rendering', () => {
  it('defines the Pi tool from the listed tool: name, hints, schema, declared output', async () => {
    const definition = octocodeToolDefinition(tool, binding);
    expect(definition).toMatchObject({ name: 'mcp__octocode__localFetch', label: 'localFetch', description: 'Read files', exposure: 'direct', annotations: { readOnlyHint: true }, parameters: tool.inputSchema });
    expect(definition).not.toHaveProperty('outputSchema');
    const withOutput = octocodeToolDefinition({ ...tool, title: 'Fetch', description: undefined, annotations: undefined, outputSchema: { type: 'object' } }, binding);
    expect(withOutput).toMatchObject({ label: 'Fetch', description: 'Fetch', outputSchema: { type: 'object' } });
    expect(withOutput).not.toHaveProperty('annotations');
    expect(octocodeToolDefinition({ name: 'x', inputSchema: {} }, binding).description).toBe('Octocode tool x');
    const out = (await definition.execute('id', { queries: [] }, undefined, undefined, stub<ExtensionToolContext>({}))) as { content: unknown };
    expect(out.content).toEqual([{ type: 'text', text: 'ok' }]);
  });

  it('resizes an oversized image to the active model\'s image limit and keeps the coordinate note', async () => {
    const original = noisyPng(512);
    const image = { type: 'image' as const, data: original.toString('base64'), mimeType: 'image/png' };
    const withImage = { ...binding, call: async () => ({ content: [image] }) };
    const run = async (model: ExtensionContext['model']) => {
      const out = (await octocodeToolDefinition(tool, withImage).execute('id', {}, undefined, undefined, stub<ExtensionToolContext>({ model }))) as { content: Array<{ type: string; text?: string; data?: string }> };
      return { shown: out.content.find((part) => part.type === 'image')!, note: out.content.find((part) => part.type === 'text')!.text! };
    };
    const small = await run(stub<NonNullable<ExtensionContext['model']>>({ inputLimits: { images: { resize: { maxWidth: 120, maxHeight: 90 } } } }));
    expect(pixelSize(small.shown.data!)).toEqual({ width: 90, height: 90 });
    expect(small.note).toContain('original 512x512, displayed at 90x90');
    expect(fs.readFileSync(/Original saved to (\S+?\.png)\./.exec(small.note)![1])).toEqual(original);
    // A model without image metadata gets the default 1568 px limit, which this image already fits: only the byte budget applies.
    const unknown = await run(undefined);
    expect(pixelSize(unknown.shown.data!)).toEqual({ width: 512, height: 512 });
    expect((await toToolResult('t', { content: [image] }, false, { maxWidth: 60, maxHeight: 60 })).content.map((part) => part.type)).toEqual(['image', 'text']);
  }, 120_000); // CPU-bound wasm resize: seconds alone, several times that on a loaded machine

  it('renders a call by what it looks for and a result as a compact block', () => {
    const definition = octocodeToolDefinition(tool, binding);
    const header = (args: object) => rendered(definition.renderCall!(args, theme, context()));
    expect(header({ queries: [{ path: '.', searchText: 'add' }] })).toContain('localFetch(add in .)');
    const batch = header({ queries: [{ path: 'a.ts' }, { path: 'b.ts' }] });
    expect(batch).toContain('localFetch(2 queries)');
    expect(batch).toContain('a.ts');
    expect(batch).toContain('b.ts');
    expect(header({})).toContain('localFetch');
    const ok = rendered(definition.renderResult!({ content: [{ type: 'text', text: 'line one\nline two' }], details: { tool: 'localFetch' } }, { expanded: false, isPartial: false }, theme, context()));
    expect(ok).toContain('line one');
    const failed = rendered(definition.renderResult!({ content: [{ type: 'text', text: 'It broke\nwith detail' }], details: { tool: 'localFetch' } }, { expanded: true, isPartial: false }, theme, context({ isError: true })));
    expect(failed).toContain('It broke');
    expect(failed).toContain('with detail');
  });
});

describe('Octocode result size budgets', () => {
  it.each([
    ['BMP text', '日'.repeat(50_000)],
    ['astral text', '😀'.repeat(25_000)],
    ['short lines', 'x\n'.repeat(25_000)],
    ['the largest measured server page (50,052 bytes)', `${'x'.repeat(50_000)}\n${'y'.repeat(51)}`],
  ])('passes a character-bounded page of %s whole, with and without a turn budget', async (_name, body) => {
    const full = `${body}\nnext:\n  continueChars: { charOffset: 50000 }\n`;
    for (const budget of [undefined, new TurnBudget()]) {
      expect(shownText(await toToolResult('localFetch', { content: [{ type: 'text', text: full }] }, false, undefined, budget))).toBe(full);
    }
  });

  it('keeps the character cap above a server page plus its header', () => {
    expect(TEXT_MAX_CHARS).toBeGreaterThan(50_100);
    expect(TEXT_MAX_CHARS).toBeLessThanOrEqual(64_000);
  });

  it('spills a part over the cap whole: the preview keeps the tail where `next` lives and names the file holding all of it', async () => {
    const huge = serverPage(120_000, '777');
    const result = await toToolResult('localSearch', { content: [{ type: 'text', text: huge }] }, false);
    const text = shownText(result);
    expect(text.length).toBeLessThanOrEqual(TEXT_MAX_CHARS);
    expect(text).toContain('continueChars: { charOffset: 777 }');
    expect(text).toMatch(/\[… \d+ lines omitted …\]/);
    expect(fileOf(text)).toBe(huge);
  });

  it('counts the results of one turn and spills what passes the budget, each whole to its own file', async () => {
    const budget = new TurnBudget();
    const pages = Array.from({ length: 6 }, (_, i) => serverPage(TEXT_MAX_CHARS - 100, String(i)));
    const results = [];
    for (const text of pages) results.push(shownText(await toToolResult('localFetch', { content: [{ type: 'text', text }] }, false, undefined, budget)));
    // Whole while the turn is under budget: three results fit in 200,000 characters.
    expect(results.slice(0, 3)).toEqual(pages.slice(0, 3));
    // The fourth is cut to what is left; the later ones to a short preview. All keep the tail and name a file with the whole text.
    for (const [i, text] of results.slice(3).entries()) {
      expect(text).toContain(`continueChars: { charOffset: ${i + 3} }`);
      expect(fileOf(text)).toBe(pages[i + 3]);
    }
    const sizes = results.map((text) => text.length);
    expect(sizes.slice(0, 4).reduce((a, b) => a + b, 0)).toBeLessThanOrEqual(TURN_MAX_CHARS + 100);
    expect(sizes[4]).toBeLessThan(8_000);
    expect(sizes[5]).toBeLessThan(8_000);
  });

  it('starts a new turn with the whole budget, and counts only what the model gets', async () => {
    const budget = new TurnBudget();
    for (let i = 0; i < 5; i++) await toToolResult('localFetch', { content: [{ type: 'text', text: serverPage(TEXT_MAX_CHARS - 100, String(i)) }] }, false, undefined, budget);
    expect(budget.limit()).toBeLessThan(8_000);
    budget.reset();
    expect(budget.limit()).toBe(TEXT_MAX_CHARS);
    const small = new TurnBudget();
    for (let i = 0; i < 100; i++) await toToolResult('t', { content: [{ type: 'text', text: 'x'.repeat(1_000) }] }, false, undefined, small);
    expect(small.limit()).toBe(TEXT_MAX_CHARS);
  });

  it('resets the turn budget when Pi starts a turn', async () => {
    const fake = fakePi();
    const mcp = registerOctocodeMcp(fake.pi);
    mcp.turn.charge(TURN_MAX_CHARS);
    expect(mcp.turn.limit()).toBeLessThan(8_000);
    await fake.fire('turn_start', { type: 'turn_start' }, fakeCtx({ cwd: tmp() }));
    expect(mcp.turn.limit()).toBe(TEXT_MAX_CHARS);
  });
});

describe('Octocode result sanitizing', () => {
  it('shows control and invisible characters as escapes in tool results: text parts and structured content, clean ones untouched', async () => {
    const dirty = 'a\u202eb\u{e0041}\u{e0042}c\u001b]0;title\u0007d\u001b[31me\u0000f';
    const result = await toToolResult('localSearch', { content: [{ type: 'text', text: dirty }, { type: 'image', data: 'x', mimeType: 'image/png' }], structuredContent: { files: [{ path: 'p', text: dirty, n: 1 }] } }, true);
    const shown = 'a\\u{202e}b\\u{e0041}\\u{e0042}c\\u{1b}]0;title\\u{7}d\\u{1b}[31me\\u{0}f';
    expect(result.content).toEqual([{ type: 'text', text: shown }, { type: 'image', data: 'x', mimeType: 'image/png' }]);
    expect(result.structuredContent).toEqual({ files: [{ path: 'p', text: shown, n: 1 }] });
    const clean = { a: ['x', { b: 'tab\there\n' }], n: 1 };
    expect(sanitizeJson(clean)).toBe(clean);
    expect(sanitizeJson([dirty, 1, null])).toEqual([shown, 1, null]);
  });

  it('keeps what a file holds visible to the model: ZWJ, BOM, bidi, tag characters and CRLF', async () => {
    const text = '1\tok = "a\u200db"\r\n2\tBOM\ufeffmid\r\n3\tzero\u200bwidth \u202eevil\u202c\n4\ttag\u{e0041}\rend';
    const result = await toToolResult('localFetch', { content: [{ type: 'text', text }] }, false);
    expect(result.content).toEqual([{ type: 'text', text: '1\tok = "a\\u{200d}b"\r\n2\tBOM\\u{feff}mid\r\n3\tzero\\u{200b}width \\u{202e}evil\\u{202c}\n4\ttag\\u{e0041}\\rend' }]);
    // Nothing raw that steers a terminal is left.
    // oxlint-disable-next-line no-control-regex -- the assertion is that no raw terminal control character survives; matching them is the point.
    expect(JSON.stringify(result.content)).not.toMatch(/[\u0000-\u0008\u000b-\u001f\u202a-\u202e\u200b-\u200d\ufeff]/u);
    // Raw CR never stands alone: only the CR of a CRLF pair, which the terminal draws as a line break, is left.
    expect(textOf(result.content[0]).replaceAll('\r\n', '')).not.toContain('\r');
  });
});
