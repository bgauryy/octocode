/**
 * The shared bulk schema defaults `format` to "tsv" — every tool that uses
 * `createRelaxedBulkQuerySchema` inherits this default, so callers who omit
 * the field get the token-efficient TSV envelope.
 */

import { describe, it, expect } from 'vitest';
import { createRelaxedBulkQuerySchema } from '../../src/scheme/localSchemaOverlay.js';
import { z } from 'zod/v4';

describe('relaxed bulk schema — format default', () => {
  const querySchema = z.object({ id: z.string().optional() });
  const bulkSchema = createRelaxedBulkQuerySchema('demoTool', querySchema);

  it('defaults format to "tsv" when the caller omits it', () => {
    const parsed = bulkSchema.parse({ queries: [{ id: 'q1' }] });
    expect(parsed.format).toBe('tsv');
  });

  it('still honors an explicit format: "json" opt-out', () => {
    const parsed = bulkSchema.parse({
      queries: [{ id: 'q1' }],
      format: 'json',
    });
    expect(parsed.format).toBe('json');
  });

  it('rejects any other format value', () => {
    const result = bulkSchema.safeParse({
      queries: [{ id: 'q1' }],
      format: 'yaml',
    });
    expect(result.success).toBe(false);
  });
});
