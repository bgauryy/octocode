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

  it('delegates regex syntax validation to native Rust', () => {
    preflightValidateRipgrepPattern({
      pattern: '(?<=foo)bar',
      fixedString: false,
      perlRegex: true,
    });

    expect(mocks.validateRipgrepPattern).toHaveBeenCalledWith(
      '(?<=foo)bar',
      false,
      true
    );
  });

  it('surfaces native regex errors without JavaScript RegExp parsing', () => {
    mocks.validateRipgrepPattern.mockReturnValue({
      valid: false,
      error: 'unclosed group',
    });

    const result = preflightValidateRipgrepPattern({
      pattern: '(',
      fixedString: false,
      perlRegex: false,
    });

    expect(result.isValid).toBe(false);
    expect(result.errors.join('\n')).toContain('unclosed group');
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
