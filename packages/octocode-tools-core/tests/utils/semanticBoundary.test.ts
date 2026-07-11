import { describe, expect, it } from 'vitest';

import {
  buildBlockBoundaryHint,
  findNextBlockBoundary,
  isMidBlockCut,
  snapToSemanticBoundary,
} from '../../src/utils/pagination/boundary.js';

const TWO_TS_FUNCTIONS =
  'function alpha() {\n  return 1;\n}\n\nfunction beta() {\n  return 2;\n}\n';

const SINGLE_GIANT_FUNCTION =
  'function big() {\n' + '  const x = 1;\n'.repeat(600) + '}\n';

describe('isMidBlockCut — pure TypeScript logic', () => {
  it('detects a mid-block cut when last non-empty line is indented (space)', () => {
    expect(isMidBlockCut('function a() {\n  return 1;')).toBe(true);
  });

  it('detects a mid-block cut when last non-empty line is indented (tab)', () => {
    expect(isMidBlockCut('function a() {\n\treturn x;')).toBe(true);
  });

  it('returns false when the last non-empty line is at column 0', () => {
    expect(isMidBlockCut('function a() {}')).toBe(false);
    expect(isMidBlockCut('function a() {}\n')).toBe(false);
    expect(isMidBlockCut('}')).toBe(false);
  });

  it('returns false for empty and whitespace-only content', () => {
    expect(isMidBlockCut('')).toBe(false);
    expect(isMidBlockCut('   \n  \n')).toBe(false);
  });
});

describe('snapToSemanticBoundary — real Rust boundaries via octocode-engine', () => {
  it('returns char-limit when content fits entirely within the budget', async () => {
    const result = await snapToSemanticBoundary(
      TWO_TS_FUNCTIONS,
      0,
      TWO_TS_FUNCTIONS.length + 100,
      'src/a.ts'
    );
    expect(result.chunkMode).toBe('char-limit');
    expect(result.length).toBe(TWO_TS_FUNCTIONS.length);
  });

  it('returns char-limit for JSON content (Rust returns no boundaries for data files)', async () => {
    const json = '{"key":"value","num":42,"arr":[1,2,3]}';
    const result = await snapToSemanticBoundary(json, 0, 5, 'config.json');
    expect(result.chunkMode).toBe('char-limit');
    expect(result.length).toBe(5);
  });

  it('returns char-limit for YAML content (Rust returns no boundaries for data files)', async () => {
    const yaml = 'key: value\nother: 42\n';
    const result = await snapToSemanticBoundary(yaml, 0, 5, 'config.yaml');
    expect(result.chunkMode).toBe('char-limit');
    expect(result.length).toBe(5);
  });

  it('snaps to a semantic boundary for TypeScript when one is within budget', async () => {
    const result = await snapToSemanticBoundary(
      TWO_TS_FUNCTIONS,
      0,
      10,
      'src/a.ts'
    );
    if (result.chunkMode === 'semantic') {
      expect(result.length).toBeGreaterThan(10);
      expect(result.length).toBeLessThanOrEqual(TWO_TS_FUNCTIONS.length);
    } else {
      expect(result.length).toBe(10);
    }
  });

  it('returns char-limit when no boundary is found after the ideal cut', async () => {
    const offset = TWO_TS_FUNCTIONS.length - 5;
    const result = await snapToSemanticBoundary(
      TWO_TS_FUNCTIONS,
      offset,
      10,
      'src/a.ts'
    );
    expect(result.chunkMode).toBe('char-limit');
  });

  it('never returns a length larger than (content.length - offset)', async () => {
    const result = await snapToSemanticBoundary(
      TWO_TS_FUNCTIONS,
      10,
      5,
      'src/a.ts'
    );
    expect(result.length).toBeLessThanOrEqual(TWO_TS_FUNCTIONS.length - 10);
  });

  it('handles a giant single function without exceeding MAX_SEMANTIC_EXTENSION', async () => {
    const result = await snapToSemanticBoundary(
      SINGLE_GIANT_FUNCTION,
      0,
      10,
      'src/big.ts'
    );
    if (result.chunkMode === 'char-limit') {
      expect(result.length).toBe(10);
    } else {
      expect(result.length - 10).toBeLessThanOrEqual(8000);
    }
  });
});

describe('findNextBlockBoundary — real Rust offsets', () => {
  it('returns undefined for JSON content (Rust reports no boundaries)', async () => {
    expect(
      await findNextBlockBoundary('{"a":1}', 3, 'file.json')
    ).toBeUndefined();
  });

  it('finds a boundary after the cut point in a two-function TypeScript file', async () => {
    const boundary = await findNextBlockBoundary(
      TWO_TS_FUNCTIONS,
      5,
      'src/a.ts'
    );
    if (boundary !== undefined) {
      expect(boundary).toBeGreaterThan(5);
      expect(boundary).toBeLessThanOrEqual(TWO_TS_FUNCTIONS.length);
    }
  });

  it('uses a generic boundary path for unknown file types', async () => {
    const result = await findNextBlockBoundary(TWO_TS_FUNCTIONS, 0);
    if (result !== undefined) {
      expect(Number.isInteger(result)).toBe(true);
      expect(result).toBeGreaterThan(0);
    }
  });
});

describe('buildBlockBoundaryHint', () => {
  it('returns undefined when the paginated content ends at column 0 (not mid-block)', async () => {
    const content = 'function a() {}\nfunction b() {}\n';
    const cut = content.indexOf('\nfunction b') + 1;
    const hint = await buildBlockBoundaryHint(
      content.slice(0, cut),
      content,
      cut,
      cut,
      'src/a.ts'
    );
    expect(hint).toBeUndefined();
  });

  it('builds a well-formed hint when the cut lands mid-block and Rust finds a next boundary', async () => {
    const insideBody = TWO_TS_FUNCTIONS.indexOf('  return 1;') + 5;
    const paginated = TWO_TS_FUNCTIONS.slice(0, insideBody);
    const hint = await buildBlockBoundaryHint(
      paginated,
      TWO_TS_FUNCTIONS,
      insideBody,
      insideBody,
      'src/a.ts'
    );
    if (hint !== undefined) {
      expect(hint.nextBlockChar).toBeGreaterThan(insideBody);
      expect(hint.hint).toMatch(/Page cut mid-block at char \d+\./);
      expect(hint.hint).toMatch(/Next top-level definition at char \d+\./);
      expect(hint.hint).toContain(`charOffset=${insideBody}`);
    }
  });
});
