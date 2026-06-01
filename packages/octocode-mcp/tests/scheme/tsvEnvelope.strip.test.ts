import { describe, expect, it } from 'vitest';
import { stripTsvEnvelope } from '../../src/scheme/tsvEnvelope.js';

describe('stripTsvEnvelope (#A1)', () => {
  it('removes presentation-only TSV keys but keeps results/hints/evidence/pagination AND base', () => {
    const out = stripTsvEnvelope({
      results: [{ a: 1 }],
      pagination: { hasMore: false },
      hints: ['h'],
      evidence: { kind: 'code' },
      format: 'tsv',
      columns: ['a'],
      rows: 'a\n1',
      base: '/x',
      shared: { k: 'v' },
    });
    // `base` is data-bearing once canonical paths are relativized — it must
    // survive into structuredContent so the model can reconstruct absolute
    // paths (abs = `${base}/${path}`). format/columns/rows/shared are stripped.
    expect(out).toEqual({
      results: [{ a: 1 }],
      pagination: { hasMore: false },
      hints: ['h'],
      evidence: { kind: 'code' },
      base: '/x',
    });
  });

  it('returns the input unchanged (same ref) when no envelope is present', () => {
    const input = { results: [], hints: ['h'] };
    expect(stripTsvEnvelope(input)).toBe(input);
  });
});
