import { describe, expect, it } from 'vitest';
import { hints } from '../../../src/tools/local_ripgrep/hints.js';
import type { HintContext } from '../../../src/types/metadata.js';

describe('local_ripgrep hints.empty', () => {
  it('emits grep-specific warning when searchEngine is grep', () => {
    const result = hints.empty({
      keywords: 'foobar',
      searchEngine: 'grep',
    } as HintContext);
    expect(result.some(h => h.includes('grep'))).toBe(true);
    expect(result.some(h => h.includes('perlRegex') || h.includes('lookahead') || h.includes('backreference'))).toBe(true);
  });

  it('does not emit grep-specific warning when searchEngine is rg', () => {
    const result = hints.empty({ keywords: 'foobar', searchEngine: 'rg' } as HintContext);
    expect(result.some(h => h.includes('grep fallback'))).toBe(false);
  });

  it('returns empty when no context provided', () => {
    const result = hints.empty({} as HintContext);
    expect(result).toEqual([]);
  });
});
