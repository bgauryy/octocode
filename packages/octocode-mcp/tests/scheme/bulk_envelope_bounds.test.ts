import { describe, it, expect } from 'vitest';

import { DIRECT_TOOL_DEFINITIONS } from '@octocodeai/config/schema';

const ALL_BULK_SCHEMAS = DIRECT_TOOL_DEFINITIONS.map(
  tool => [tool.name, tool.inputSchema] as const
);

const DEFAULT_MAX_QUERIES = 5;

describe('bulk envelope numeric bounds', () => {
  describe.each(ALL_BULK_SCHEMAS)('%s', (_name, schema) => {
    const maxQueries = DEFAULT_MAX_QUERIES;
    const baseQueries = [{ id: 'q1' }];

    it('parses with minimal queries (envelope accepted, per-query errors ok)', () => {
      const result = schema.safeParse({ queries: baseQueries });
      if (!result.success) {
        const envelopeErrors = result.error.issues.filter(
          i => i.path.length === 1 && i.path[0] === 'queries'
        );
        expect(envelopeErrors).toHaveLength(0);
      }
    });

    it('does not expose responseOffset or responseLength', () => {
      const result = schema.safeParse({ queries: baseQueries });
      if (result.success) {
        expect(result.data).not.toHaveProperty('responseOffset');
        expect(result.data).not.toHaveProperty('responseLength');
      }
    });

    it(`rejects more than ${maxQueries} queries`, () => {
      const result = schema.safeParse({
        queries: Array.from({ length: maxQueries + 1 }, (_, index) => ({
          id: `q${index + 1}`,
        })),
      });
      expect(result.success).toBe(false);
      if (!result.success) {
        // Envelope-level size cap must be reported. Tools with a plain object
        // envelope surface it on the `queries` path; union envelopes may
        // surface an `invalid_union` at the root instead — both
        // are valid rejections of an oversized batch.
        const rejectsOnQueries = result.error.issues.some(
          issue => issue.path.join('.') === 'queries'
        );
        const rejectsAtRoot = result.error.issues.some(
          issue => issue.path.length === 0
        );
        expect(rejectsOnQueries || rejectsAtRoot).toBe(true);
      }
    });
  });
});
