import { describe, it, expect } from 'vitest';

import { LocalFetchContentQuerySchema } from '../../../octocode-tools-core/src/tools/local_fetch_content/scheme.js';
import { LocalRipgrepQuerySchema } from '../../../octocode-tools-core/src/tools/local_ripgrep/scheme.js';
import { FileContentQueryLocalSchema } from '../../../octocode-tools-core/src/tools/github_fetch_content/scheme.js';
import { NpmSearchBulkQueryLocalSchema } from '../../../octocode-tools-core/src/tools/package_search/scheme.js';

describe('LocalFetchContentQuerySchema mutual-exclusion', () => {
  const baseQuery = { path: 'src/foo.ts' };

  it('rejects fullContent=true together with matchString', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
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
    const result = LocalFetchContentQuerySchema.safeParse({
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
    const result = LocalFetchContentQuerySchema.safeParse({
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
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      fullContent: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts matchString alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      matchString: 'foo',
    });
    expect(result.success).toBe(true);
  });

  it('accepts startLine+endLine alone', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
      ...baseQuery,
      startLine: 10,
      endLine: 20,
    });
    expect(result.success).toBe(true);
  });

  it('accepts fullContent=false with matchString', () => {
    const result = LocalFetchContentQuerySchema.safeParse({
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

  it('rejects an inverted startLine/endLine range', () => {
    const result = FileContentQueryLocalSchema.safeParse({
      ...baseQuery,
      startLine: 20,
      endLine: 10,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages).toContain(
        'endLine must be greater than or equal to startLine'
      );
    }
  });
});

describe('LocalRipgrepQuerySchema mutex checks', () => {
  const baseQuery = { keywords: 'foo', path: '/repo' };

  it('rejects filesOnly=true together with filesWithoutMatch=true', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
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
    const result = LocalRipgrepQuerySchema.safeParse({
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
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      filesOnly: true,
    });
    expect(result.success).toBe(true);
  });

  it('accepts fixedString=true alone', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      fixedString: true,
    });
    expect(result.success).toBe(true);
  });

  it('rejects countLinesPerFile together with countMatchesPerFile', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...baseQuery,
      countLinesPerFile: true,
      countMatchesPerFile: true,
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      const messages = result.error.issues.map(i => i.message).join('\n');
      expect(messages.toLowerCase()).toMatch(/mutually exclusive/);
    }
  });
});

describe('NpmSearch schema', () => {
  it('accepts packageName (npm only)', () => {
    const result = NpmSearchBulkQueryLocalSchema.safeParse({
      queries: [{ packageName: 'react' }],
    });
    expect(result.success).toBe(true);
  });

  it('rejects when packageName is missing', () => {
    const result = NpmSearchBulkQueryLocalSchema.safeParse({
      queries: [{}],
    });
    expect(result.success).toBe(false);
  });
});
