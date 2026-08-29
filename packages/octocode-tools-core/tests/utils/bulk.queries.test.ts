import { describe, expect, it } from 'vitest';

import { computeQueryTimeout } from '../../src/utils/response/bulk/queries.js';

describe('computeQueryTimeout', () => {
  it('honors a tool minimum for a single long-running query', () => {
    expect(computeQueryTimeout(1, 3, 130_000)).toBe(130_000);
  });
});
