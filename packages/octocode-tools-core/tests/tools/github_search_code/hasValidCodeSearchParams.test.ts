import { describe, expect, it } from 'vitest';

import { hasValidCodeSearchParams } from '../../../src/tools/github_search_code/execution.js';

describe('ghSearchCode hasValidCodeSearchParams', () => {
  it('accepts a language-only query (regression: language was omitted from the validity check)', () => {
    expect(hasValidCodeSearchParams({ language: 'rust' })).toBe(true);
  });

  it('accepts an extension-only query', () => {
    expect(hasValidCodeSearchParams({ extension: 'rs' })).toBe(true);
  });

  it('accepts a keywords-only query', () => {
    expect(hasValidCodeSearchParams({ keywords: ['useState'] })).toBe(true);
  });

  it('rejects an empty query with no search term or scope filter', () => {
    expect(hasValidCodeSearchParams({})).toBe(false);
  });

  it('rejects a query with only whitespace keywords', () => {
    expect(hasValidCodeSearchParams({ keywords: ['   '] })).toBe(false);
  });
});
