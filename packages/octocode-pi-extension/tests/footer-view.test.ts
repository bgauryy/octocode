import { visibleWidth } from '../src/tui/width.js';
import assert from 'node:assert/strict';
import { test } from 'vitest';
import { renderFooterView, type FooterViewProps } from '../src/tui/footer-view.js';


const FULL: FooterViewProps = {
  rows: [
    [{ text: 'Thinking…' }, { text: 'context ▓▓▓▓▓▓░░ 76% · 152k/200k' }],
    [{ text: 'plan 3/6' }, { text: 'task 4 Verify footer' }],
    [{ text: 'main (7 changed)' }, { text: 'model openai/gpt-5.6' }, { text: 'github ✓' }, { text: 'perm default +2' }, { text: '/commands' }],
    [{ text: 'turn 8 · 14s' }, { text: 'session 12m 8s' }, { text: 'initial ~18.4k (sys 9.2k · mcp 5/42 · skills 11)' }],
  ],
  agents: [
    { label: 'agent builder', state: 'running', elapsed: '14s', task: 'Unify state ownership', doing: 'running footer tests' },
    { label: 'agent reviewer', state: 'blocked', elapsed: '9s', task: 'Review footer', attention: true },
    { label: 'agent old', state: 'killed', elapsed: '1s' },
  ],
};

test('footer retains each semantic state category at every supported width', () => {
  for (const width of [28, 40, 64, 96, 140]) {
    const lines = renderFooterView(FULL, { width });
    const body = lines.join('\n');
    assert.ok(lines.length >= 5, 'status, plan, identity, metrics, and agents stay visible');
    assert.match(body, /Thinking/);
    assert.match(body, /plan 3\/6/);
    assert.match(body, /task 4/);
    assert.match(body, /main/);
    assert.match(body, /github/);
    assert.match(body, /agent builder.*running/);
    assert.match(body, /doing running footer/);
    assert.match(body, /agent reviewer.*blocked/);
    assert.doesNotMatch(body, /agent old|killed/);
    for (const line of lines) assert.ok(visibleWidth(line) <= width, `${width}: ${line}`);
  }
});

test('footer removes empty segments and preserves caller-owned row order', () => {
  const lines = renderFooterView({
    rows: [[
      { text: 'main' },
      { text: 'session 0s' },
      { text: 'context ▓▓▓▓▓▓▓▓ 96% · 192k/200k', attention: true },
      { text: 'failed 2', attention: true },
    ]],
    agents: [],
  }, { width: 36 });
  assert.ok(lines.length >= 2);
  assert.match(lines[0] ?? '', /^main.*session/);
  assert.match(lines.join('\n'), /context/);
  assert.match(lines.join('\n'), /failed 2/);
  assert.doesNotMatch(lines.join('\n'), /0\/200k \(0%\)/);
});

test('footer shows every visible worker and never hides active work behind an overflow count', () => {
  const lines = renderFooterView({
    rows: [[{ text: 'Thinking…' }, { text: 'ctx 120k/200k (60%)' }]],
    agents: Array.from({ length: 7 }, (_, index) => ({
      label: `agent worker-${index + 1}`,
      state: 'running',
      elapsed: `${index + 1}s`,
    })),
  }, { width: 80 });
  assert.equal(lines.length, 8, 'header + every non-killed worker');
  assert.match(lines[0]!, /Thinking….*120k\/200k/);
  assert.match(lines.at(-1)!, /agent worker-7.*running/);
});
