import assert from 'node:assert/strict';
import test from 'node:test';
import { oneFactor, pairwise } from './pairwise.mjs';

const notOneX = row => !(row.a === 1 && row.b === 'x');
const axes = [{ key: 'a', values: [1, 2, 3] }, { key: 'b', values: ['x', 'y'] }, { key: 'c', values: [true, false] }, { key: 'd', values: ['p', 'q', 'r'] }];

test('every valid pair meets in a row, with far fewer rows than the product', () => {
  const rows = pairwise(axes, () => true);
  for (const [i, x] of axes.entries()) for (const y of axes.slice(i + 1)) for (const vx of x.values) for (const vy of y.values) {
    assert.ok(rows.some(row => row[x.key] === vx && row[y.key] === vy), `${x.key}=${vx} ${y.key}=${vy}`);
  }
  assert.ok(rows.length < 3 * 2 * 2 * 3);
});

test('a constraint removes the pair and never appears in a row', () => {
  const rows = pairwise(axes, notOneX);
  assert.ok(rows.every(notOneX));
  assert.ok(!rows.some(row => row.a === 1 && row.b === 'x'));
  assert.ok(rows.some(row => row.a === 2 && row.b === 'x'));
});

test('each value stands alone in one row', () => {
  assert.equal(oneFactor(axes, () => true).length, axes.reduce((sum, axis) => sum + axis.values.length, 0));
  assert.deepEqual(oneFactor([{ key: 'a', values: [1, 2] }], row => row.a !== 1), [{ a: 2 }]);
});

test('no pair across axes means no rows', () => {
  assert.deepEqual(pairwise([{ key: 'a', values: [1] }], () => true), []);
});
