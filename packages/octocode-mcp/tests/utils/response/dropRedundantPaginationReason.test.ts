import { describe, it, expect } from 'vitest';
import { dropRedundantPaginationReason } from '../../../src/utils/response/bulk.js';

// #B2: a result-page cursor hint already conveys "there's more", so the generic
// "Result pagination has more results." reason is redundant and gets dropped —
// but only when such a cursor hint is present, and other reasons are preserved.
describe('dropRedundantPaginationReason (#B2)', () => {
  it('drops the generic reason when a "Next: page=" cursor hint exists', () => {
    const out = dropRedundantPaginationReason(
      {
        kind: 'pr',
        complete: false,
        reason: 'Result pagination has more results.',
      },
      ['Page 1/10 (showing 1-3 of 346 PRs). Next: page=2']
    );
    expect(out.reason).toBeUndefined();
    expect(out.complete).toBe(false);
  });

  it('keeps other reasons, dropping only the redundant one', () => {
    const out = dropRedundantPaginationReason(
      {
        kind: 'pr',
        complete: false,
        reason:
          'Result pagination has more results.; Result hints report capped output.',
      },
      ['Page 2/3. Next: page=3']
    );
    expect(out.reason).toBe('Result hints report capped output.');
  });

  it('does nothing when there is no cursor hint', () => {
    const ev = {
      kind: 'pr' as const,
      complete: false,
      reason: 'Result pagination has more results.',
    };
    expect(dropRedundantPaginationReason(ev, ['some unrelated hint'])).toBe(ev);
  });

  it('leaves char-pagination reasons untouched (different cursor family)', () => {
    const ev = {
      kind: 'calls' as const,
      complete: false,
      reason: 'One or more query-level output pages have more data.',
    };
    const out = dropRedundantPaginationReason(ev, [
      'Next page: use charOffset=8000 to continue',
    ]);
    expect(out.reason).toBe(
      'One or more query-level output pages have more data.'
    );
  });
});
