import assert from 'node:assert/strict';
import { it } from 'vitest';
import { assertBudgets } from '../scripts/bench-web-scan.mjs';

const row = (budgetMs: number, first: number, worst: number) => ({ name: 'supplied sample', budgetMs, first, worst });

it('explicit scan budgets retain strict 200ms and 2000ms thresholds without measuring time', () => {
  assert.doesNotThrow(() => assertBudgets([row(200, 199.999, 199.999), row(2000, 1999.999, 1999.999)]));
  for (const limit of [200, 2000]) {
    assert.throws(() => assertBudgets([row(limit, limit, 0)]), /supplied sample/);
    assert.throws(() => assertBudgets([row(limit, 0, limit)]), /supplied sample/);
    assert.throws(() => assertBudgets([row(limit, NaN, 0)]), /supplied sample/);
  }
  assert.doesNotThrow(() => assertBudgets([{ name: 'observation only', first: 10000, worst: 10000 }]));
});
