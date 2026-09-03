import assert from 'node:assert/strict';
import { test } from 'vitest';
import { renderFooterView, type FooterViewProps } from '../src/tui/footer-view.js';
import { visibleWidth } from '../src/tools/render-helpers.js';

const FULL: FooterViewProps = {
  identity: [
    { text: 'main (7 changed)' }, { text: 'model openai/gpt-5.6' }, { text: 'github ✓' },
    { text: 'perm default +2' }, { text: '/commands' },
  ],
  metrics: [
    { text: 'context ▓▓▓▓▓▓░░ 76% · 152k/200k' }, { text: 'turn 8 · 14s' },
    { text: 'session 12m 8s' }, { text: 'initial ~18.4k (sys 9.2k · mcp 5/42 · skills 11)' },
    { text: 'agents 3 (2 live)' },
  ],
  agents: [
    { label: 'agent builder (a1b2c3)', model: 'gpt-5.6', task: 'Unify TUI rendering', planStep: '2. Build shared components', state: 'running', elapsed: '14s', doing: 'editing footer-view.ts' },
    { label: 'agent reviewer (d4e5f6)', model: 'claude-sonnet', task: 'Review tool output', state: 'blocked', elapsed: '9s', doing: 'waiting for index.ts', attention: true },
    { label: 'agent tester (f7a8b9)', state: 'done', elapsed: '5s' },
  ],
  shortcuts: [
    { text: 'shift+tab think' }, { text: 'ctrl+shift+a perm' }, { text: 'ctrl+l model' },
    { text: 'ctrl+o tools' }, { text: 'esc stop' },
  ],
};

test('footer component renders every state at every supported width without overflow', () => {
  for (const width of [28, 40, 64, 96, 140]) {
    const lines = renderFooterView(FULL, { width });
    const body = lines.join('\n');
    assert.match(body, /context/);
    assert.match(body, /builder/);
    assert.match(body, /reviewer/);
    assert.match(body, /tester/);
    assert.match(body, /gpt-5\.6/);
    if (width >= 64) assert.match(body, /Unify TUI rendering/);
    if (width >= 96) assert.match(body, /Build shared components/);
    for (const line of lines) assert.ok(visibleWidth(line) <= width, `${width}: ${line}`);
  }
});

test('footer removes empty placeholder rows and prioritizes attention metrics', () => {
  const lines = renderFooterView({
    identity: [{ text: 'main' }],
    metrics: [
      { text: 'session 0s' },
      { text: 'context ▓▓▓▓▓▓▓▓ 96% · 192k/200k', attention: true },
      { text: 'failed 2', attention: true },
    ],
    agents: [],
    shortcuts: [],
  }, { width: 36 });
  assert.equal(lines.some((line) => line === ''), false);
  assert.match(lines[1] ?? '', /context|failed/);
  assert.doesNotMatch(lines.join('\n'), /0\/200k \(0%\)/);
});
