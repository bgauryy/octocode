import { describe, it, expect } from 'vitest';

import {
  FetchContentQuerySchema,
  RipgrepQuerySchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  FileContentQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';

describe('FetchContentQuerySchema mutual-exclusion', () => {
  const baseQuery = { path: 'src/foo.ts' };

  it('rejects fullContent=true together with matchString', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
      matchString: 'foo',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive|matchstring/);
    }
  });

  it('rejects fullContent=true together with startLine/endLine', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/startline\/endline/);
    }
  });

  it('rejects matchString together with startLine/endLine', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/matchstring/);
    }
  });

  it('accepts fullContent=true alone', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts matchString alone', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });

  it('accepts startLine+endLine alone', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(true);
  });

  it('accepts fullContent=false with matchString', () => {
    const result = FetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: false,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });
});

describe('FileContentQueryLocalSchema (github) three-mode mutual exclusion', () => {
  const baseQuery = { owner: 'o', repo: 'r', path: 'src/foo.ts' };

  it('rejects fullContent=true together with matchString', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      matchString: 'foo',
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive/);
    }
  });

  it('rejects fullContent=true together with startLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      startLine: 10,
    });
    expect(result.success).toBe(false);
  });

  it('rejects fullContent=true together with endLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
      endLine: 20,
    });
    expect(result.success).toBe(false);
  });

  it('rejects matchString together with startLine/endLine', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(false);
  });

  it('accepts fullContent=true alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      fullContent: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts matchString alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });

  it('accepts startLine+endLine alone', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(true);
  });
});

describe('RipgrepQuerySchema mutex checks', () => {
  const baseQuery = { pattern: 'foo', path: '/repo' };

  it('rejects filesOnly=true together with filesWithoutMatch=true', () => {
    const result = RipgrepQuerySchema.safeParse({
      ...baseQuery,
      filesOnly: true,
      filesWithoutMatch: true,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive/);
    }
  });

  it('rejects fixedString=true together with perlRegex=true', () => {
    const result = RipgrepQuerySchema.safeParse({
      ...baseQuery,
      fixedString: true,
      perlRegex: true,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive/);
    }
  });

  it('accepts filesOnly=true alone', () => {
    const result = RipgrepQuerySchema.safeParse({
      ...baseQuery,
      filesOnly: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts fixedString=true alone', () => {
    const result = RipgrepQuerySchema.safeParse({
      ...baseQuery,
      fixedString: true,
    });
    expect(result.success).toBe(true);
  });

  it('allows count + countMatches together (warning, not error)', () => {
    const result = RipgrepQuerySchema.safeParse({
      ...baseQuery,
      count: true,
      countMatches: true,
    });
    expect(result.success).toBe(true);
  });
});

describe('PackageSearch schema', () => {
  it('accepts name omitted ecosystem (npm only)', () => {
    const result = PackageSearchBulkQueryLocalSchema.safeParse({
      queries: [{ name: 'react' }],
    });
    expect(result.success).toBe(true);
  });

  it('accepts packageName as an alias for name', () => {
    const result = PackageSearchBulkQueryLocalSchema.safeParse({
      queries: [{ packageName: 'zod' }],
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect(result.data.queries[0].name).toBe('zod');
    }
  });
});
