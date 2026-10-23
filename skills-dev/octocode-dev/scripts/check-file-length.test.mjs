import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compare, initial, isChecked, MAX_LINES, physicalLines, shrink } from './check-file-length.mjs';

const OVER = MAX_LINES + 50;

test('physical lines count every line, blank and comment lines included', () => {
  assert.equal(physicalLines(''), 0);
  assert.equal(physicalLines('a\n\n// c\n'), 3);
  assert.equal(physicalLines('a\nb'), 2, 'a last line without a newline still counts');
});

test('source files are checked; generated files and fixtures are not', () => {
  for (const file of ['src/a.ts', 'src/a.tsx', 'bin/x.cjs', 'x.mjs', 'x.js', 'crates/a/src/lib.rs', 'x.mts']) {
    assert.ok(isChecked(file), file);
  }
  for (const file of [
    'src/toolTypes.generated.ts',
    'contract/tool_types.rs',
    'contract/tool-contract.json',
    'tests/fixtures/big.ts',
    'local/e2e/seeded/x.ts',
    'README.md',
  ]) {
    assert.ok(!isChecked(file), file);
  }
});

test('a new file over the limit fails; a baselined file may stay or shrink but not grow', () => {
  const counts = new Map([
    ['new.ts', OVER],
    ['kept.ts', OVER],
    ['shrunk.ts', OVER - 10],
    ['grew.rs', OVER + 1],
    ['small.ts', 10],
  ]);
  const baseline = { 'kept.ts': OVER, 'shrunk.ts': OVER, 'grew.rs': OVER };
  assert.deepEqual(
    compare(counts, baseline).map(failure => failure.split(':')[0]),
    ['new.ts', 'grew.rs']
  );
});

test('a baselined file within the limit, or deleted, is stale', () => {
  const counts = new Map([['fixed.ts', MAX_LINES]]);
  assert.deepEqual(
    compare(counts, { 'fixed.ts': OVER, 'gone.rs': OVER }).map(failure => failure.split(':')[0]),
    ['fixed.ts', 'gone.rs']
  );
});

test('--write only shrinks: stale entries leave, counts drop, nothing is added', () => {
  const counts = new Map([
    ['new.ts', OVER],
    ['shrunk.ts', OVER - 10],
    ['fixed.ts', 12],
  ]);
  assert.deepEqual(shrink(counts, { 'shrunk.ts': OVER, 'fixed.ts': OVER, 'gone.ts': OVER }), { 'shrunk.ts': OVER - 10 });
  assert.deepEqual(initial(counts), { 'new.ts': OVER, 'shrunk.ts': OVER - 10 }, 'the first baseline records every file over the limit');
});
