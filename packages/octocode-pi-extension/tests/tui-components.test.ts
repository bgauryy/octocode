import { visibleWidth } from '../src/tui/width.js';
import assert from 'node:assert/strict';
import { test } from 'vitest';
import { CURSOR_MARKER } from '@earendil-works/pi-tui';
import { renderFrame, renderInlineRows, renderStack, renderToolView } from '../src/tui/components.js';


test('renderFrame closes and aligns every border at narrow and wide widths', () => {
  for (const width of [2, 3, 18, 32, 64, 100]) {
    const lines = renderFrame({
      title: '◆ Input needed · 2 of 3',
      body: ['Which renderer should own the footer?', '› Zustand-backed component'],
      footer: 'enter confirm · esc cancel',
    }, { width });
    assert.ok(lines[0]?.startsWith('╭'));
    assert.ok(lines[0]?.endsWith('╮'));
    assert.ok(lines.at(-1)?.startsWith('╰'));
    assert.ok(lines.at(-1)?.endsWith('╯'));
    for (const line of lines) assert.equal(visibleWidth(line), width, `${width}: ${line}`);
    for (const line of lines.slice(1, -1)) {
      assert.ok(line.startsWith('│'));
      assert.ok(line.endsWith('│'));
    }
  }
});

test('all frame edge permutations remain closed, including the one-cell fallback', () => {
  assert.deepEqual(renderFrame({ title: 'x', body: ['y'], footer: 'z' }, { width: 1 }), ['╭']);
  const closed = renderFrame({ title: 'title', body: ['body'], footer: 'footer' }, { width: 24 });
  assert.equal(closed.length, 3);
  assert.ok(closed[0]!.endsWith('╮'));
  assert.ok(closed[1]!.endsWith('│'));
  assert.ok(closed[2]!.endsWith('╯'));
  for (const line of closed) assert.equal(visibleWidth(line), 24);
});

test('interactive component frames preserve the cursor marker and close the right rail', () => {
  const lines = renderFrame({
    title: 'Input needed', body: [`answer ${CURSOR_MARKER}draft`], footer: 'enter confirm',
  }, { width: 28 });
  assert.ok(lines[1]!.includes(CURSOR_MARKER));
  assert.ok(lines[0]!.endsWith('╮'));
  assert.ok(lines[1]!.endsWith('│'));
  assert.ok(lines[2]!.endsWith('╯'));
  for (const line of lines) assert.equal(visibleWidth(line.replace(CURSOR_MARKER, '')), 28);
});

test('renderFrame remains cell-perfect with ANSI, emoji, CJK, and tabs', () => {
  const lines = renderFrame({
    title: '\x1b[36m界面 ⭐\x1b[0m',
    body: ['agent\tworking 👨‍👩‍👧‍👦', '長い説明'],
    footer: 'ready ✓',
  }, { width: 30 });
  assert.equal(lines.length, 4);
  for (const line of lines) assert.equal(visibleWidth(line), 30);
});

test('renderInlineRows wraps complete segments instead of clipping the important tail', () => {
  const rows = renderInlineRows({
    segments: [
      { text: 'context ▓▓▓▓▓▓▓░ 92% · 184k/200k', attention: true },
      { text: 'agents 4 (2 live)' },
      { text: 'blocked 1', attention: true },
      { text: 'failed 1', attention: true },
    ],
  }, { width: 34 });
  assert.ok(rows.length > 1);
  assert.match(rows.join('\n'), /blocked 1/);
  assert.match(rows.join('\n'), /failed 1/);
  for (const line of rows) assert.ok(visibleWidth(line) <= 34);
});

test('renderStack composes component output without blank padding', () => {
  assert.deepEqual(renderStack({ sections: [['one'], [], ['two', 'three']] }, { width: 20 }), ['one', 'two', 'three']);
});

test('renderToolView composes the same semantic slots for requests and results', () => {
  const request = renderToolView({
    name: 'web',
    state: 'request',
    segments: [
      { text: 'search', token: 'bright' },
      { text: 'terminal UI', token: 'link' },
    ],
    body: [{ text: 'Compare every renderer', token: 'muted' }],
  }, { width: 80 });
  const result = renderToolView({
    name: 'web',
    state: 'success',
    segments: [
      { text: '8 results', token: 'count' },
      { text: 'https://example.test', token: 'link' },
    ],
    hint: 'ctrl+o to expand evidence',
  }, { width: 80 });

  assert.match(request[0]!, /^◇ web · search · terminal UI$/);
  assert.equal(request[1], '  Compare every renderer');
  assert.match(result[0]!, /^✓ web · 8 results · https:\/\/example\.test$/);
  assert.equal(result[1], '  ctrl+o to expand evidence');
});

test('renderToolView maps state to meaningful glyphs and remains width-safe', () => {
  const expected = {
    request: '◇',
    running: 'spinner',
    success: '✓',
    error: '✗',
    warning: '!',
    neutral: '–',
  } as const;
  for (const [state, glyph] of Object.entries(expected)) {
    const lines = renderToolView({
      name: 'veryLongToolName',
      state: state as keyof typeof expected,
      status: state === 'running' ? 'running…' : undefined,
      segments: [{ text: 'a deliberately long custom tool summary', token: 'path' }],
      body: [{ text: 'full output remains in tool context', token: 'muted' }],
    }, { width: 24 });
    if (glyph === 'spinner') assert.match(lines[0]!, /^[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏]/);
    else assert.ok(lines[0]!.startsWith(glyph), `${state}: ${lines[0]}`);
    for (const line of lines) assert.ok(visibleWidth(line) <= 24, `${state}: ${line}`);
  }
});

test('every primitive is width-safe across pathological terminal sizes', () => {
  for (const width of [1, 2, 8, 24, 80, 160]) {
    const lines = [
      ...renderInlineRows({ segments: [{ text: 'model gpt-5.6' }, { text: 'context 199k/200k', attention: true }] }, { width }),
      ...renderStack({ sections: [['Plan'], ['▶ running task'], ['Awareness · verify-debt 2']] }, { width }),
      ...renderFrame({ title: 'Tool result', body: ['✓ [0] success', '✗ [1] failed'], footer: 'parallel' }, { width }),
    ];
    for (const line of lines) assert.ok(visibleWidth(line) <= width, `${width}: ${line}`);
  }
});
