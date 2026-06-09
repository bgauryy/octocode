import { describe, it, expect } from 'vitest';

import { LocalRipgrepQuerySchema } from '../../src/tools/local_ripgrep/scheme.js';
import { LocalFindFilesQuerySchema } from '../../src/tools/local_find_files/scheme.js';

/**
 * The `type` param historically meant 4 unrelated things across tools
 * (file|dir / f|d / ripgrep-lang / lsp-query-kind). To de-collide the two
 * local tools, `localSearchCode.type` was renamed to `langType` and
 * `localFindFiles.type` to `entryType`. There is no `type` alias — the raw
 * upstream `type` field is omitted from the public schema, so each tool has
 * exactly one name for the filter. Execution maps the agent-facing name to
 * the internal `type` the command builders read.
 */
describe('localSearchCode langType (no type alias)', () => {
  const base = { pattern: 'foo', path: 'src' };

  it('accepts langType', () => {
    const result = LocalRipgrepQuerySchema.safeParse({
      ...base,
      langType: 'ts',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { langType?: string }).langType).toBe('ts');
    }
  });

  it('does not expose `type` on the public schema (stripped, not honored)', () => {
    const result = LocalRipgrepQuerySchema.safeParse({ ...base, type: 'ts' });
    // Zod objects strip unknown keys rather than failing, so parse still
    // succeeds — but `type` must NOT survive as a recognized field.
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { type?: string }).type).toBeUndefined();
    }
  });
});

describe('localFindFiles entryType (no type alias)', () => {
  const base = { path: 'src' };

  it('accepts entryType', () => {
    const result = LocalFindFilesQuerySchema.safeParse({
      ...base,
      entryType: 'd',
    });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { entryType?: string }).entryType).toBe('d');
    }
  });

  it('does not expose `type` on the public schema (stripped, not honored)', () => {
    const result = LocalFindFilesQuerySchema.safeParse({ ...base, type: 'f' });
    expect(result.success).toBe(true);
    if (result.success) {
      expect((result.data as { type?: string }).type).toBeUndefined();
    }
  });
});
