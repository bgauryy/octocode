import fs from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { BrowserSession } from '../src/browser/session-state.js';
import { formatChanges, formatSnapshot, snapshot } from '../src/browser/snapshot.js';

const sessionOf = (data: unknown) => ({ page: { untilDialog: <T>(work: Promise<T>) => work, evaluate: async () => data } }) as unknown as BrowserSession;

describe('snapshot spill', () => {
  it('saves the full snapshot with controls and hidden characters as visible escapes: nothing raw to steer a terminal, nothing dropped', async () => {
    const items = Array.from({ length: 1_000 }, (_, index) => `[${index + 1}] button "Action \u202e${index + 1}"`);
    const out = await snapshot(sessionOf({ title: 'Big \u001b]0;pwned\u0007', url: 'https://example.com', text: `page\u200d text ${'word '.repeat(10_000)}\u{e0041}`, items }));
    const file = /Full snapshot \(every element and all text\): (\S+);/.exec(out)![1]!.replace(/^~/, process.env['HOME']!);
    const saved = fs.readFileSync(file, 'utf8');
    expect(saved).toContain('[1000] button "Action \\u{202e}1000"');
    expect(saved).toContain('# Big \\u{1b}]0;pwned\\u{7}');
    expect(saved).toContain('page\\u{200d} text ');
    expect(saved).toContain('\\u{e0041}');
    expect(saved).not.toMatch(/[\u001b\u0007\u202e\u200d\u{e0041}]/u);
  });

  it('keeps CRLF page text as it is and escapes a lone carriage return', async () => {
    const items = Array.from({ length: 1_000 }, (_, index) => `[${index + 1}] button "Action ${index + 1}"`);
    const out = await snapshot(sessionOf({ title: 'T', url: 'https://example.com', text: `crlf\r\nline\rlone ${'word '.repeat(10)}`, items }));
    const file = /Full snapshot \(every element and all text\): (\S+);/.exec(out)![1]!.replace(/^~/, process.env['HOME']!);
    const saved = fs.readFileSync(file, 'utf8');
    expect(saved).toContain('crlf\r\nline\\rlone ');
  });
});

describe('formatSnapshot', () => {
  it('keeps the element list first and fills the rest of 8 KB with page text', () => {
    const items = Array.from({ length: 1_000 }, (_, index) => `[${index + 1}] button "Action number ${index + 1}"`);
    const out = formatSnapshot({ title: 'Big', url: 'https://example.com', text: 'word '.repeat(10_000), items });
    expect(out.length).toBeLessThanOrEqual(8_200);
    expect(out.indexOf('## Elements')).toBeLessThan(out.indexOf('## Text'));
    expect(out).toContain('[1] button');
    expect(out).toMatch(/more elements; click by number still works/);
    expect(out).toMatch(/more characters cut\]$/);
    const small = formatSnapshot({ title: 'T', url: 'u', text: 'hello', items: [] });
    expect(small).toBe('# T\nu\n\n## Elements\n(none)\n\n## Text\nhello');
  });

  it('says when the page text was cut in the page (200,000 characters), in the view and the saved full snapshot', () => {
    const data = { title: 'Huge', url: 'https://example.com', text: 'x'.repeat(200_000), textTotal: 250_000, items: [] };
    for (const out of [formatSnapshot(data), formatSnapshot(data, Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER)]) {
      expect(out).toContain('## Text (first 200000 of 250000 characters; scroll or wait for a specific text to reach the rest)\n');
    }
    expect(formatSnapshot({ ...data, textTotal: 200_000 })).toContain('## Text\n');
  });

  it('names the locale the page rendered for and lists footer elements compactly', () => {
    const footer = Array.from({ length: 200 }, (_, index) => `[${index + 10}] Footer link ${index}`);
    const out = formatSnapshot({ title: 'Plans', url: 'https://wix.test/plans', text: '₪ 327 mo', items: ['[1] button "Start"'], footer, locale: 'page lang en · browser en-US · time zone Asia/Jerusalem · currency ₪×41' });
    expect(out).toMatch(/^# Plans\nhttps:\/\/wix\.test\/plans\nLocale: page lang en · browser en-US · time zone Asia\/Jerusalem · currency ₪×41\n\n## Elements\n\[1\] button "Start"\n\n## Footer \(200\)\n\[10\] Footer link 0 · \[11\] Footer link 1/);
    expect(out).toMatch(/more characters cut; click by number still works\]\n\n## Text\n₪ 327 mo$/);
  });
});

describe('formatChanges', () => {
  const page = { title: 'Shop', url: 'https://shop.test/', text: 'Cart\nEmpty', items: ['[1] button "Add"', '[2] input "Qty" value="1"'] };
  it('reports new text, and only the elements that are new, changed or gone (numbers are stable)', () => {
    expect(formatChanges(page, { ...page, text: 'Cart\n1 item' })).toBe('# Shop\nhttps://shop.test/ (same page)\n\n## Elements\nUnchanged (2).\n\n## New text (1 line gone)\n1 item');
    const changed = formatChanges(page, { ...page, items: ['[2] input "Qty" value="3"', '[5] button "Checkout"'] });
    expect(changed).toContain('## Elements (new or changed)\n[2] input "Qty" value="3"\n[5] button "Checkout"\nNo longer shown: [1]\n\n## New text\nNo new text.');
    expect(formatChanges(page, { ...page, text: `Cart\n${'new line\n'.repeat(1_000)}` })).toMatch(/more characters cut; take a snapshot to see the whole page\]$/);
  });
});
