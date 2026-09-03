import assert from 'node:assert/strict';
import { test } from 'vitest';
import { MultiSelectList, multiSelectKeyAction } from '../src/tools/multi-select-list.js';
import type { MultiSelectTheme } from '../src/tools/multi-select-list.js';

const theme = { fg: (c: string, t: string) => '<' + c + '>' + t + '</' + c + '>' } as unknown as MultiSelectTheme;

test('multi-select toggles at the cursor and reports values in item order', () => {
  const list = new MultiSelectList([{ value: 'a' }, { value: 'b' }, { value: 'c' }]);

  assert.equal(list.toggle(), true);
  list.moveCursor(1);
  list.moveCursor(1);
  assert.equal(list.cursor, 2);
  assert.equal(list.toggle(), true);
  assert.deepEqual(list.selectedValues(), ['a', 'c']);
  assert.equal(list.selectionCount(), 2);

  // Toggling a selected item removes it.
  assert.equal(list.toggle(), true);
  assert.deepEqual(list.selectedValues(), ['a']);
});

test('max gates additional toggles and min gates confirm', () => {
  const list = new MultiSelectList([{ value: 'a' }, { value: 'b' }, { value: 'c' }], { min: 1, max: 2 });

  assert.equal(list.canConfirm(), false, 'below min must not confirm');
  assert.equal(list.toggle(0), true);
  assert.equal(list.canConfirm(), true);
  assert.equal(list.toggle(1), true);
  assert.equal(list.toggle(2), false, 'toggle beyond max is a gated no-op');
  assert.deepEqual(list.selectedValues(), ['a', 'b']);
  assert.equal(list.canConfirm(), true);

  // Removing always works, then the freed slot can be re-used.
  assert.equal(list.toggle(1), true);
  assert.equal(list.toggle(2), true);
  assert.deepEqual(list.selectedValues(), ['a', 'c']);
});

test('initial values preselect (respecting max) and cursor clamps to bounds', () => {
  const list = new MultiSelectList(
    [{ value: 'a' }, { value: 'b' }, { value: 'c' }],
    { max: 2, initial: ['b', 'c', 'a', 'nope'] },
  );
  assert.deepEqual(list.selectedValues(), ['b', 'c'], 'initial stops at max and ignores unknown values');

  assert.equal(list.moveCursor(-5), 0);
  assert.equal(list.moveCursor(99), 2);
  assert.equal(list.moveCursor(-1), 1);
});

test('constraints clamp to satisfiable values', () => {
  const list = new MultiSelectList([{ value: 'a' }], { min: 5, max: 2 });
  assert.equal(list.min, 1, 'min clamps to item count');
  list.toggle();
  assert.equal(list.canConfirm(), true);

  const inverted = new MultiSelectList([{ value: 'a' }, { value: 'b' }, { value: 'c' }], { min: 2, max: 1 });
  assert.equal(inverted.max, 2, 'max never undercuts min');
  inverted.toggle(0);
  inverted.toggle(1);
  assert.equal(inverted.canConfirm(), true);

  const empty = new MultiSelectList([], { min: 3 });
  assert.equal(empty.canConfirm(), true, 'empty list confirms with zero selections');
  assert.deepEqual(empty.selectedValues(), []);
});

test('render shows cursor marker, checkboxes, focused preview, descriptions and footer', () => {
  const list = new MultiSelectList(
    [
      { value: 'a', label: 'Alpha', preview: 'line1\nline2' },
      { value: 'b', label: 'Beta', description: 'second' },
    ],
    { min: 1 },
  );
  list.toggle();

  assert.deepEqual(list.render(60, theme), [
    '<accent>› [x] Alpha</accent>',
    '<dim>    │ line1</dim>',
    '<dim>    │ line2</dim>',
    '  [ ] Beta<muted> — second</muted>',
    '<dim>1 selected · min 1 · enter to confirm</dim>',
  ]);

  // Preview follows the cursor: focusing Beta hides Alpha's preview.
  list.moveCursor(1);
  assert.deepEqual(list.render(60, theme), [
    '  [x] Alpha',
    '<accent>› [ ] Beta</accent><muted> — second</muted>',
    '<dim>1 selected · min 1 · enter to confirm</dim>',
  ]);
});

test('render footer warns while below min and shows max constraint', () => {
  const list = new MultiSelectList([{ value: 'a' }, { value: 'b' }, { value: 'c' }], { min: 2, max: 3 });
  const lines = list.render(60, theme);
  assert.equal(lines.at(-1), '<dim>0 selected · min 2 · max 3 · select 2 more</dim>');

  list.toggle(0);
  list.toggle(1);
  assert.equal(list.render(60, theme).at(-1), '<dim>2 selected · min 2 · max 3 · enter to confirm</dim>');
});

test('render clips rows, previews and footer to the given width', () => {
  const list = new MultiSelectList([{ value: 'a', label: 'Alphabetical', preview: 'a very long preview line' }]);
  list.toggle();
  const lines = list.render(10, theme);
  assert.equal(lines[0], '<accent>› [x] Alp…</accent>');
  assert.equal(lines[1], '<dim>    │ a v…</dim>');
  assert.equal(lines.at(-1), '<dim>1 selecte…</dim>');
});

test('render works without a theme (plain text fallback)', () => {
  const list = new MultiSelectList([{ value: 'a', label: 'Alpha' }]);
  assert.deepEqual(list.render(40), ['› [ ] Alpha', '0 selected · enter to confirm']);
});

test('multiSelectKeyAction maps the overlay keymap and ignores everything else', () => {
  assert.equal(multiSelectKeyAction('\x1b[A'), 'up');
  assert.equal(multiSelectKeyAction('\x10'), 'up');
  assert.equal(multiSelectKeyAction('\x1b[B'), 'down');
  assert.equal(multiSelectKeyAction('\x0e'), 'down');
  assert.equal(multiSelectKeyAction(' '), 'toggle');
  assert.equal(multiSelectKeyAction('\r'), 'confirm');
  assert.equal(multiSelectKeyAction('\n'), 'confirm');
  assert.equal(multiSelectKeyAction('\x1b'), 'cancel');
  assert.equal(multiSelectKeyAction('\x03'), 'cancel');
  assert.equal(multiSelectKeyAction('x'), undefined);
  assert.equal(multiSelectKeyAction('\x1b[C'), undefined);
});

test('render windows long lists around the cursor with more-markers', () => {
  const list = new MultiSelectList(
    Array.from({ length: 25 }, (_, i) => ({ value: `v${i + 1}`, label: `item-${String(i + 1).padStart(2, '0')}` })),
  );
  const first = list.render(80, undefined, 10).join('\n');
  assert.match(first, /item-01/);
  assert.match(first, /↓ 15 more/, 'hidden tail advertised');
  assert.doesNotMatch(first, /item-25/, 'rows beyond the window are not painted');

  for (let i = 0; i < 20; i++) list.moveCursor(1);
  const scrolled = list.render(80, undefined, 10).join('\n');
  assert.match(scrolled, /↑ \d+ more/, 'hidden head advertised after scrolling');
  assert.match(scrolled, /item-21/);

  // Small lists render fully with no markers.
  const small = new MultiSelectList([{ value: 'a' }, { value: 'b' }]);
  assert.doesNotMatch(small.render(80).join('\n'), /more/);
});
