import { describe, expect, it, vi, beforeEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  validateRipgrepPattern: vi.fn(),
}));

vi.mock('../../../src/utils/contextUtils.js', () => ({
  contextUtils: {
    validateRipgrepPattern: mocks.validateRipgrepPattern,
  },
}));

const { preflightValidateRipgrepPattern } =
  await import('../../../src/tools/local_ripgrep/patternValidation.js');

describe('preflightValidateRipgrepPattern', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.validateRipgrepPattern.mockReturnValue({ valid: true });
  });

  it('does not compile regexes during warning-only preflight', () => {
    const result = preflightValidateRipgrepPattern({
      pattern: '(?<=foo)bar',
      fixedString: false,
      perlRegex: true,
    });

    expect(result.isValid).toBe(true);
    expect(mocks.validateRipgrepPattern).not.toHaveBeenCalled();
  });

  it('leaves regex compilation to the native execution boundary', () => {
    const result = preflightValidateRipgrepPattern({
      pattern: '(',
      fixedString: false,
      perlRegex: false,
    });

    expect(result.isValid).toBe(true);
    expect(mocks.validateRipgrepPattern).not.toHaveBeenCalled();
  });

  it('keeps literal and lookaround guidance warnings', () => {
    const literal = preflightValidateRipgrepPattern({ pattern: 'src/foo.ts' });
    expect(literal.warnings.join('\n')).toContain('regex:"fixed"');

    const lookaround = preflightValidateRipgrepPattern({
      pattern: '(?<=foo)bar',
    });
    expect(lookaround.warnings.join('\n')).toContain(
      'requires perlRegex: true'
    );
  });
});
