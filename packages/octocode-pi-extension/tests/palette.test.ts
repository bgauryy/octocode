import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  TOKEN,
  paint,
  paintUi,
  colorEnabled,
  hyperlinksEnabled,
  hyperlink,
  isHttpUrl,
  contextGauge,
} from '../src/tui/palette.js';

const theme = { fg: (c: string, t: string) => `<${c}>${t}</${c}>`, bold: (t: string) => `<b>${t}</b>` };

test('semantic tokens map to shipped theme color keys', () => {
  assert.equal(TOKEN.brand, 'accent');
  assert.equal(TOKEN.brandAlt, 'syntaxOperator');
  assert.equal(TOKEN.path, 'mdCode');
  assert.equal(TOKEN.link, 'mdLink');
  assert.equal(TOKEN.count, 'text');
  assert.equal(TOKEN.diffAdd, 'toolDiffAdded');
});

test('paint uses theme token and falls back to raw text', () => {
  assert.equal(paint(theme, 'path', 'src/a.ts'), '<mdCode>src/a.ts</mdCode>');
  assert.equal(paint(undefined, 'path', 'src/a.ts'), 'src/a.ts');
});

test('paintUi tolerates an uninitialized noninteractive theme getter', () => {
  const ui = { get theme(): never { throw new Error('Theme not initialized'); } };
  assert.equal(paintUi(ui, 'warning', 'plain status'), 'plain status');
  assert.equal(paintUi({ theme }, 'path', 'src/a.ts'), '<mdCode>src/a.ts</mdCode>');
});

test('colorEnabled honors NO_COLOR, FORCE_COLOR, and requires a TTY by default', () => {
  // No env override → gated on stdout.isTTY so raw SGR never leaks into
  // piped/redirected output. Vitest runs without a TTY, so the default is
  // whatever the real stream reports.
  assert.equal(colorEnabled({}), process.stdout.isTTY === true);
  assert.equal(colorEnabled({ NO_COLOR: '1' }), false);
  assert.equal(colorEnabled({ NO_COLOR: '' }), process.stdout.isTTY === true); // empty = not set per convention
  assert.equal(colorEnabled({ FORCE_COLOR: '1' }), true); // explicit force wins over non-TTY
  assert.equal(colorEnabled({ NO_COLOR: '1', FORCE_COLOR: '1' }), false); // NO_COLOR wins
});

test('hyperlinksEnabled follows color unless explicitly overridden', () => {
  assert.equal(hyperlinksEnabled({ FORCE_COLOR: '1' }), true);
  assert.equal(hyperlinksEnabled({ NO_COLOR: '1' }), false);
  assert.equal(hyperlinksEnabled({ NO_COLOR: '1', OCTOCODE_HYPERLINKS: '1' }), true);
  assert.equal(hyperlinksEnabled({ OCTOCODE_HYPERLINKS: '0', FORCE_COLOR: '1' }), false);
});

test('hyperlink wraps in OSC 8 when enabled, plain otherwise', () => {
  const linked = hyperlink('https://x.dev', 'x', { FORCE_COLOR: '1' });
  assert.equal(linked, '\x1b]8;;https://x.dev\x07x\x1b]8;;\x07');
  assert.equal(hyperlink('https://x.dev', 'x', { NO_COLOR: '1' }), 'x');
  assert.equal(hyperlink('', 'x', { FORCE_COLOR: '1' }), 'x'); // empty url → plain
});

test('isHttpUrl matches only http(s) urls', () => {
  assert.equal(isHttpUrl('https://github.com/o/r'), true);
  assert.equal(isHttpUrl('http://x'), true);
  assert.equal(isHttpUrl('src/a.ts'), false);
  assert.equal(isHttpUrl('ftp://x'), false);
});

test('contextGauge fills proportionally and clamps', () => {
  assert.equal(contextGauge(0, 8).bar, '░░░░░░░░');
  assert.equal(contextGauge(100, 8).bar, '▓▓▓▓▓▓▓▓');
  assert.equal(contextGauge(50, 8).bar, '▓▓▓▓░░░░');
  assert.equal(contextGauge(150, 8).pct, 100); // clamp high
  assert.equal(contextGauge(-10, 8).pct, 0); // clamp low
  assert.equal(contextGauge(50, 8).bar.length, 8);
});

test('contextGauge severity shifts success → warning → error', () => {
  assert.equal(contextGauge(10).token, 'success');
  assert.equal(contextGauge(80).token, 'warning');
  assert.equal(contextGauge(95).token, 'error');
});
