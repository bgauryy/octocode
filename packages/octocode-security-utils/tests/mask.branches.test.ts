import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('maskSensitiveData - zero-length match branch coverage (line 38)', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should handle zero-length matches gracefully without infinite loop', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /(?=a)/g,
          name: 'zero-length-test',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'aaaa';
    const result = maskSensitiveData(input);

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  it('should handle optional pattern groups that can match empty', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /SECRET=([a-z]*)?/gi,
          name: 'optional-test',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'SECRET=abc';
    const result = maskSensitiveData(input);

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  it('should process text with multiple potential zero-length match positions', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /(?<=a)(?=b)/g,
          name: 'position-test',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'ab ab ab';
    const result = maskSensitiveData(input);

    expect(result).toBeDefined();
    expect(result).toBe('ab ab ab');
  });
});

describe('maskSensitiveData - patternMap accuracy fallback', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should fallback to medium accuracy when pattern has undefined accuracy', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /SECRET="([^"]{16,})"/gi,
          name: 'test-pattern',
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'SECRET="abcdefghijklmnop"';
    const result = maskSensitiveData(input);

    expect(result).toBeDefined();
    expect(result).not.toBe(input);
  });
});

describe('maskSensitiveData - edge cases for match array access', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should handle pattern array index bounds correctly', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /FIRST_PATTERN="[^"]{10,}"/gi,
          name: 'first',
          matchAccuracy: 'high' as const,
        },
        {
          regex: /SECOND_PATTERN="[^"]{10,}"/gi,
          name: 'second',
          matchAccuracy: 'medium' as const,
        },
        {
          regex: /THIRD_PATTERN="[^"]{10,}"/gi,
          name: 'third',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'THIRD_PATTERN="1234567890"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
  });
});

describe('maskSensitiveData - overlapping matches branch (lines 52-56)', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should skip overlapping matches and keep only the first one', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /SECRET_VALUE="[^"]+"/gi,
          name: 'full-secret',
          matchAccuracy: 'high' as const,
        },
        {
          regex: /VALUE="[^"]+"/gi,
          name: 'partial-value',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'PREFIX SECRET_VALUE="1234567890" SUFFIX';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
    expect(result.startsWith('PREFIX ')).toBe(true);
    expect(result.endsWith(' SUFFIX')).toBe(true);
  });

  it('should trigger overlapping branch when inner pattern starts inside outer match', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /OUTER_INNER_END/gi,
          name: 'outer',
          matchAccuracy: 'high' as const,
        },
        {
          regex: /INNER_END/gi,
          name: 'inner',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'TEST OUTER_INNER_END TEST';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
  });

  it('should correctly handle adjacent non-overlapping matches', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /TOKEN="[^"]{10,}"/gi,
          name: 'token',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'TOKEN="1234567890" TOKEN="abcdefghij"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    const asteriskCount = (result.match(/\*/g) || []).length;
    expect(asteriskCount).toBeGreaterThan(10);
  });

  it('should handle matches that start at the same position', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /SECRET_LONG="[^"]{20,}"/gi,
          name: 'long',
          matchAccuracy: 'high' as const,
        },
        {
          regex: /SECRET_LONG="[^"]{10,}"/gi,
          name: 'short',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'SECRET_LONG="12345678901234567890"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
  });
});

describe('maskSensitiveData - nonOverlapping array iteration (lines 60-69)', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should correctly process multiple non-overlapping matches in reverse', async () => {
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          regex: /KEY\d="[^"]{10,}"/gi,
          name: 'numbered-key',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'KEY1="1234567890" KEY2="abcdefghij" KEY3="zyxwvutsrq"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
    const asteriskCount = (result.match(/\*/g) || []).length;
    expect(asteriskCount).toBeGreaterThan(20);
  });
});
