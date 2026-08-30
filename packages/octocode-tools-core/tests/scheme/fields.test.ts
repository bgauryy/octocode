import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import {
  clampedInt,
  createRelaxedBulkQuerySchema,
} from '../../src/scheme/fields.js';

describe('shared schema fields', () => {
  it('clamps finite integer input into the configured bounds', () => {
    const field = clampedInt(1, 5);

    expect(field.parse(-10)).toBe(1);
    expect(field.parse(10)).toBe(5);
    expect(field.parse(3)).toBe(3);
  });

  it('rejects non-numeric and non-integer input instead of coercing', () => {
    const field = clampedInt(1, 5);

    expect(field.safeParse('3').success).toBe(false);
    expect(field.safeParse(3.7).success).toBe(false);
    expect(field.safeParse(Number.NaN).success).toBe(false);
  });

  it('adds response pagination fields and clamps their bounds', () => {
    const schema = createRelaxedBulkQuerySchema(
      z.object({
        value: z.string(),
      }),
      { maxQueries: 2 }
    );

    const parsed = schema.parse({
      queries: [{ value: 'alpha' }],
      responseCharOffset: -1,
      responseCharLength: 100_000,
    });

    expect(parsed.responseCharOffset).toBe(0);
    expect(parsed.responseCharLength).toBe(50_000);
  });

  it('accepts identical query values because response indexes provide correlation', () => {
    const schema = createRelaxedBulkQuerySchema(
      z.object({ value: z.string() })
    );

    const result = schema.safeParse({
      queries: [{ value: 'same' }, { value: 'same' }],
    });

    expect(result.success).toBe(true);
    if (result.success) expect(result.data.queries).toHaveLength(2);
  });
});
