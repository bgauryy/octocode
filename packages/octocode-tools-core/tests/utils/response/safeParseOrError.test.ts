import { describe, expect, it } from 'vitest';
import { z } from 'zod';

import { safeParseOrError } from '../../../src/utils/response/error.js';

const schema = z.object({
  path: z.string(),
  limit: z.number().int().min(1),
});

describe('safeParseOrError (finding 7)', () => {
  it('returns ok with parsed data on valid input', () => {
    const outcome = safeParseOrError(schema, { path: '/x', limit: 5 });
    expect(outcome.ok).toBe(true);
    if (outcome.ok) {
      expect(outcome.data).toEqual({ path: '/x', limit: 5 });
    }
  });

  it('returns the actionable schema message without a redundant prefix by default', () => {
    const outcome = safeParseOrError(schema, { path: 123 });
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) {
      expect(outcome.error.status).toBe('error');
      expect(String(outcome.error.error)).not.toMatch(/^Validation error: /);
    }
  });

  it('joins multiple issue messages with "; "', () => {
    const outcome = safeParseOrError(schema, {});
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) {
      const msg = String(outcome.error.error);
      expect(msg).toContain(';');
    }
  });

  it('adds the prefix only when prefix=true', () => {
    const outcome = safeParseOrError(schema, { path: 123 }, { prefix: true });
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) {
      expect(String(outcome.error.error)).toMatch(/^Validation error: /);
    }
  });
});
