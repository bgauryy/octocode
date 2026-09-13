import { describe, expect, it } from 'vitest';
import { LocalRipgrepQuerySchema } from '@octocodeai/octocode-core/schema';
import { regexErrorRecovery } from '../../../src/tools/local_ripgrep/regexErrorRecovery.js';

describe('regexErrorRecovery native boundary', () => {
  const query = LocalRipgrepQuerySchema.parse({
    path: '/repo',
    searchText: '(',
  });

  it.each([
    new Error('Permission denied: regex parse error:'),
    new Error('PCRE2: error matching: JIT stack limit reached'),
    'search aborted',
  ])('leaves non-compilation failures unchanged: %s', error => {
    expect(regexErrorRecovery(error, query)).toEqual({});
  });

  it('does not reinterpret fixed-string failures as regex errors', () => {
    expect(
      regexErrorRecovery(
        new Error('regex parse error:\nerror: unclosed group'),
        {
          ...query,
          regex: 'fixed',
        }
      )
    ).toEqual({});
  });
});
