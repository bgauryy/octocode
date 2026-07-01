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

  it('teaches field-predicate lane scoping and boolean sugar', () => {
    const full = oqlSchemaText();
    // field predicates are files-lane only; agents must learn this BEFORE
    // hitting unsupportedPredicate.
    expect(full).toMatch(/field predicates evaluate file attributes/i);
    expect(full).toContain('booleanSugar');
    for (const sugar of [
      'and:',
      'or:',
      'noneOf:',
      'xor:',
      'oneOf:',
      'invert:true',
    ]) {
      expect(full).toContain(sugar);
    }
  });

  it('enumerates controls.search.sort values with the files-only gate', () => {
    // The schema doc is JSON-serialized, so unescape inner quotes to assert
    // on the human-readable enum text.
    const full = oqlSchemaText().replace(/\\"/g, '"');
    expect(full).toContain(
      'sort?:"relevance"|"matchCount"|"path"|"modified"|"accessed"|"created"|"size"|"name"'
    );
    expect(full).toMatch(/"size"\/"name" apply to target:"files" only/);
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
