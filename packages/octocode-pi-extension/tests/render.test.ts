import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { formatAgo, formatDuration, formatSpan } from '../src/shared/format.js';
import { wholeSeconds } from '../src/shared/util.js';
import { appKey, clip, clipColumns, expandKey, livePaint, plural, queryTarget, resultBlock, statusColor, toolHeader } from '../src/shared/render.js';
import { stopRowTimers, type ToolRow } from '../src/shared/tool-timing.js';
import { visibleWidth } from '@earendil-works/pi-tui';
import { rendered, theme } from './fake-pi.js';

const row = (overrides: Record<string, unknown> = {}) => ({ state: {} as ToolRow, invalidate: vi.fn(), executionStarted: false, isPartial: true, isError: false, expanded: false, ...overrides });

afterEach(() => vi.useRealTimers());

describe('format helpers', () => {
  it.each([
    [0, '0ms'],
    [850, '850ms'],
    [1_200, '1.2s'],
    [9_400, '9.4s'],
    [59_000, '59s'],
    [65_000, '1m05s'],
    [900_000, '15m'],
    [3_780_000, '1h03m'],
    [7_200_000, '2h'],
    [-5, '0ms'],
  ])('formatDuration(%d, true) = %s', (ms, text) => expect(formatDuration(ms, true)).toBe(text));

  it.each([
    [0, '0s ago'],
    [45_000, '45s ago'],
    [125_000, '2m ago'],
    [3_600_000, '1h ago'],
    [19_200_000, '5h20m ago'],
    [90_000_000, '1d1h ago'],
    [86_400_000 * 3, '3d ago'],
    [-5, '0s ago'],
  ])('formatAgo(%d) = %s: the one relative-time format, coarser than formatDuration', (ms, text) => expect(formatAgo(ms)).toBe(text));

  it('formatSpan is formatAgo without the suffix; wholeSeconds rounds', () => {
    expect(formatSpan(125_000)).toBe('2m');
    expect(wholeSeconds(1_499)).toBe(1);
    expect(wholeSeconds(1_500)).toBe(2);
  });

  it('clipColumns cuts by terminal columns, never inside a character, and leaves ASCII as clip does', () => {
    expect(clipColumns('abcdef', 4)).toBe(clip('abcdef', 4));
    expect(clipColumns('abc', 4)).toBe('abc');
    const wide = clipColumns('漢字漢字漢字', 7);
    expect(wide).toBe('漢字漢…');
    expect(visibleWidth(wide)).toBeLessThanOrEqual(7);
    const emoji = clipColumns('👨‍👩‍👧👨‍👩‍👧👨‍👩‍👧', 5);
    expect(visibleWidth(emoji)).toBeLessThanOrEqual(5);
    expect(emoji.endsWith('…')).toBe(true);
    expect(clipColumns('x', 0)).toBe('');
  });

  it('pluralizes, clips and falls back to ctrl+o for the expand key', () => {
    expect(plural(1, 'line')).toBe('1 line');
    expect(plural(2, 'tool use')).toBe('2 tool uses');
    expect(plural(3, 'query', 'queries')).toBe('3 queries');
    expect(clip('abcdef', 4)).toBe('abc…');
    expect(clip('\u001b]0;evil\u0007ok\nsecond', 10)).toBe('ok');
    expect(expandKey()).toMatch(/\S/);
  });

  it('finds the search text of a query before its path', () => {
    expect(queryTarget({ path: '.', searchText: 'add' })).toBe('add in .');
    expect(queryTarget({ owner: 'o', repo: 'r', keywords: ['a', 'b'] })).toBe('o/r a b');
    expect(queryTarget({ owner: 'o', repo: 'r', path: 'src' })).toBe('o/r/src');
    expect(queryTarget({ path: 'src/index.ts' })).toBe('src/index.ts');
  });
});

describe('toolHeader', () => {
  it('shows ○ before execution without a time', () => {
    expect(rendered(toolHeader(theme, row(), 'Bash', 'ls'))).toBe('○ Bash(ls)');
  });

  it('folds batch sub-lines while collapsed: three, then +N more', () => {
    const sub = Array.from({ length: 10 }, (_, index) => `query ${index}`);
    const collapsed = rendered(toolHeader(theme, row(), 'localSearch', '10 queries', { lines: sub })).split('\n');
    expect(collapsed).toEqual(['○ localSearch(10 queries)', '  query 0', '  query 1', '  query 2', `  … +7 more (${expandKey()} to expand)`]);
    expect(rendered(toolHeader(theme, row({ expanded: true }), 'localSearch', '10 queries', { lines: sub })).split('\n')).toHaveLength(11);
    // Folding one line would not save one.
    expect(rendered(toolHeader(theme, row(), 'x', '4 queries', { lines: sub.slice(0, 4) })).split('\n')).toHaveLength(5);
  });

  it('caches its lines per width until the next set, so pi-tui sees the same strings each frame', () => {
    const header = toolHeader(theme, row(), 'Bash', 'ls');
    const first = header.render(80);
    expect(header.render(80)).toBe(first);
    expect(header.render(60)).not.toBe(first);
    const again = toolHeader(theme, row({ lastComponent: header }), 'Bash', 'pwd');
    expect(again).toBe(header);
    expect(rendered(again)).toBe('○ Bash(pwd)');
  });

  it('marks errors with ✗ and clips to the width while keeping the time', () => {
    const header = toolHeader(theme, row({ isPartial: false, isError: true, durationMs: 1_500 }), 'Web', 'x'.repeat(100), { meta: 'bg' });
    const text = rendered(header, 40);
    expect(text.startsWith('✗ Web(xxx')).toBe(true);
    expect(text.endsWith(') · bg · 1.5s')).toBe(true);
    expect(visibleWidth(text)).toBeLessThanOrEqual(40);
  });

  it('sanitizes the name, summary and sub-lines', () => {
    const text = rendered(toolHeader(theme, row(), 'N\u001b[31m', 'a\u001b]8;;http://x\u0007b', { lines: ['sub'] }));
    expect(text).toBe('○ N(ab)\n  sub');
  });

  it('shows native duration immediately for restored rows without a result-driven redraw', async () => {
    const context = row({ isPartial: false, durationMs: 42 });
    expect(rendered(toolHeader(theme, context, 'Read', 'a.ts'))).toBe('● Read(a.ts) · 42ms');
    resultBlock(theme, context, { summary: 'Read 3 lines' });
    await Promise.resolve();
    expect(context.invalidate).not.toHaveBeenCalled();
  });

  it('does not invent completed durations when Pi has no recorded timing', () => {
    const context = row({ isPartial: false, state: { startedAt: Date.now() - 5_000, durationMs: 42 } });
    expect(rendered(toolHeader(theme, context, 'Read', 'a.ts'))).toBe('● Read(a.ts)');
    expect(rendered(toolHeader(theme, { ...context, durationMs: 0 }, 'Read', 'a.ts'))).toBe('● Read(a.ts) · 0ms');
  });
});

describe('resultBlock', () => {
  const body = Array.from({ length: 10 }, (_, index) => `line ${index}`).join('\n');

  it('keeps every line within a width narrower than the body indent', () => {
    const block = resultBlock(theme, { expanded: true }, { summary: 'page', body: 'first line\nsecond line' });
    for (const width of [1, 3, 4]) for (const line of block.render(width)) expect(visibleWidth(line)).toBeLessThanOrEqual(width);
  });

  it('skips blank lines in the collapsed preview but keeps them when expanded', () => {
    const text = '# Title\n\nSource: x\n\n\npara\nmore';
    expect(rendered(resultBlock(theme, {}, { summary: 'page', body: text }))).toBe('  ⎿  page\n     # Title\n     Source: x\n     para\n     … +4 lines (ctrl+o to expand)');
    expect(rendered(resultBlock(theme, { expanded: true }, { summary: 'page', body: text })).split('\n')).toHaveLength(8);
  });

  it('collapses to the summary, three lines and an expand hint', () => {
    expect(rendered(resultBlock(theme, row({ isPartial: false }), { summary: '10 lines', body }))).toBe(
      ['  ⎿  10 lines', '     line 0', '     line 1', '     line 2', `     … +7 lines (${expandKey()} to expand)`].join('\n'),
    );
  });

  it('previews the tail with the hint first', () => {
    expect(rendered(resultBlock(theme, row({ isPartial: false }), { summary: 'exit 0', body, tail: true, max: 2 }))).toBe(
      ['  ⎿  exit 0', `     … +8 lines (${expandKey()} to expand)`, '     line 8', '     line 9'].join('\n'),
    );
  });

  it('shows everything when expanded, wrapped to the width', () => {
    const text = rendered(resultBlock(theme, row({ isPartial: false, expanded: true }), { summary: 's', body: `${body}\n${'y'.repeat(50)}` }), 30);
    expect(text.split('\n')).toHaveLength(1 + 10 + 2);
    expect(text).not.toContain('to expand');
  });

  it('prefixes errors, clips long lines and names the spill file', () => {
    const text = rendered(resultBlock(theme, row({ isPartial: false, isError: true }), { summary: 'exit 1', body: 'z'.repeat(300), spill: '/tmp/out.txt' }), 60);
    const [summary, line, spill] = text.split('\n');
    // Collapsed, the saved path is behind the expand key; expanded it shows, home- or cwd-relative.
    expect(summary).toBe('  ⎿  Error: exit 1 (ctrl+o to expand)');
    expect(visibleWidth(line!)).toBeLessThanOrEqual(60);
    expect(line!.replace(/\u001b\[[0-9;]*m/g, '').endsWith('…')).toBe(true);
    expect(spill).toBeUndefined();
    const open = (spillPath: string, label?: string) => rendered(resultBlock(theme, row({ isPartial: false, expanded: true }), { summary: 'exit 0', body: 'ok', spill: spillPath, ...(label ? { spillLabel: label } : {}) }), 200).split('\n').at(-1);
    expect(open('/tmp/out.txt')).toBe('     saved: /tmp/out.txt');
    expect(open(path.join(os.homedir(), 'x', 'out.txt'), 'log')).toBe(`     log: ~${path.sep}${path.join('x', 'out.txt')}`);
    expect(open(path.join(process.cwd(), 'out.txt'))).toBe('     saved: out.txt');
  });

  it('strips escape sequences from the body and keeps pre-styled lines as given', () => {
    expect(rendered(resultBlock(theme, row(), { summary: 'ok', body: '\u001b[31mred\u001b[0m\n\u202eevil' }))).toBe('  ⎿  ok\n     red\n     evil');
    expect(rendered(resultBlock(theme, row(), { summary: '', lines: ['+ a', '- b'] }))).toBe('  ⎿  + a\n     - b');
  });
});

describe('resultBlock colouring', () => {
  it('colours only the lines a collapsed preview shows, and counts the hidden rest', () => {
    let painted = 0;
    const counting = { ...(theme as object), fg: (_color: string, text: string) => ((painted += 1), text) } as never;
    const body = Array.from({ length: 1_000 }, (_, index) => `line ${index}`).join('\n');
    const text = rendered(resultBlock(counting, row({ isPartial: false }), { summary: 'exit 0', body, tail: true }));
    expect(text).toContain('… +997 lines');
    expect(painted).toBeLessThan(10);
    expect(rendered(resultBlock(theme, row({ isPartial: false }), { summary: 'exit 0', body: 'a\nb', totalLines: 50, tail: true }))).toContain('… +48 lines');
  });
});

describe('statusColor', () => {
  it('lets the footer repaint a status in the live theme', () => {
    const old = { fg: (color: string, text: string) => `<old ${color}>${text}` };
    const status = statusColor({ mode: 'tui', ui: { theme: old } } as never, 'accent', '✉ 2 queued');
    expect(status).toBe('<old accent>✉ 2 queued');
    expect(livePaint(status, { fg: (color: string, text: string) => `<new ${color}>${text}` } as never)).toBe('<new accent>✉ 2 queued');
    expect(livePaint('plain', { fg: () => 'x' } as never)).toBe('plain');
    expect(statusColor({ mode: 'rpc', ui: { theme: old } } as never, 'accent', 'x')).toBe('x');
  });
});

describe('interrupted rows and expandable prefaces', () => {
  const colored = { ...(theme as object), fg: (color: string, text: string) => `<${color}>${text}` } as never;

  it('draws an Esc abort as ◼ interrupted in the warning colour, and redraws a header that drew ✗', async () => {
    const state: ToolRow = {};
    const invalidate = vi.fn();
    const context = { isPartial: false, isError: true, executionStarted: true, state, invalidate };
    expect(rendered(toolHeader(colored, context, 'Bash', 'sleep 9') as never)).toContain('<error>✗');
    const body = rendered(resultBlock(colored, context, { summary: 'Operation aborted' }) as never);
    expect(body).toContain('<warning>Operation aborted');
    expect(body).not.toContain('Error:');
    await Promise.resolve();
    expect(invalidate).toHaveBeenCalledTimes(1);
    expect(rendered(toolHeader(colored, context, 'Bash', 'sleep 9') as never)).toContain('<warning>◼');
    // A real failure keeps its Error prefix and ✗.
    const failed = rendered(resultBlock(colored, { isPartial: false, isError: true, state: {} }, { summary: 'exit 1' }) as never);
    expect(failed).toContain('Error: exit 1');
  });

  it('counts a preface apart from hidden body lines and shows it only when expanded', () => {
    const preface = ['Prompt', 'line 1', 'line 2'];
    const collapsed = rendered(resultBlock(theme, { isPartial: false, isError: false }, { summary: 'Done', body: 'a\nb', preface, prefaceLabel: 'prompt' }) as never);
    expect(collapsed).toBe('  ⎿  Done\n     a\n     b\n     … prompt (ctrl+o to expand)');
    const more = rendered(resultBlock(theme, { isPartial: false, isError: false }, { summary: 'Done', body: '1\n2\n3\n4\n5', preface, prefaceLabel: 'prompt' }) as never);
    expect(more).toContain('… +2 lines · prompt (ctrl+o to expand)');
    const open = rendered(resultBlock(theme, { isPartial: false, isError: false, expanded: true }, { summary: 'Done', body: 'a', preface }) as never);
    expect(open).toBe('  ⎿  Done\n     Prompt\n     line 1\n     line 2\n     a');
  });

  it('falls back to Pi default keys and stops every live row ticker on demand', () => {
    expect(appKey('app.thinking.toggle', 'ctrl+t')).toBe('ctrl+t');
    vi.useFakeTimers();
    try {
      const invalidate = vi.fn();
      toolHeader(theme, { executionStarted: true, isPartial: true, state: {}, invalidate }, 'Bash', 'x');
      vi.advanceTimersByTime(1_100);
      expect(invalidate).toHaveBeenCalledTimes(1);
      stopRowTimers();
      vi.advanceTimersByTime(5_000);
      expect(invalidate).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });
});
