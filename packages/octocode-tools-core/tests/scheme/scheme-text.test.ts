import { describe, expect, it } from 'vitest';

import {
  oqlSchemaText,
  oqlCompactSchemeText,
  oqlCompactSchemeJson,
} from '../../src/oql/schemeText.js';
import { ACTIVE_TARGETS } from '../../src/oql/types.js';

describe('oqlSchemaText (full --scheme)', () => {
  it('retains the advanced research surface', () => {
    const full = oqlSchemaText();
    for (const field of [
      'research',
      'graph',
      'predicates',
      'providerUnindexed',
      'materialize',
      'artifacts',
    ]) {
      expect(full).toContain(field);
    }
  });
});

describe('oqlCompactSchemeText (--scheme --compact)', () => {
  const compact = oqlCompactSchemeText();

  it('teaches the source -> target -> recipe route', () => {
    expect(compact).toContain('SOURCE');
    expect(compact).toContain('TARGET');
    expect(compact).toContain('COMMON RECIPES');
    for (const target of ACTIVE_TARGETS) {
      expect(compact).toContain(target);
    }
  });

  it('surfaces the npm and remote-file-read recipes (Haiku gaps)', () => {
    expect(compact).toContain('--target packages');
    expect(compact).toContain('--content-view exact');
  });

  it('distinguishes references from callers', () => {
    expect(compact).toContain('references');
    expect(compact).toContain('callers');
    expect(compact).toMatch(/callers[\s\S]*incoming calls/);
  });

  it('points back to the full schema for advanced flows', () => {
    expect(compact).toContain('search --scheme');
  });

  it('is meaningfully shorter than the full schema', () => {
    const compactLines = compact.split('\n').length;
    const fullLines = oqlSchemaText().split('\n').length;
    expect(compactLines).toBeLessThan(fullLines / 2);
  });

  it('has a compact machine-readable form', () => {
    const parsed = JSON.parse(oqlCompactSchemeJson()) as {
      kind: string;
      targets: Array<{ target: string }>;
    };
    expect(parsed.kind).toBe('octocode.search.compactScheme');
    expect(parsed.targets.map(entry => entry.target)).toContain('graph');
  });
});
