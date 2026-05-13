/**
 * Branch coverage tests for mask.ts
 *
 * This file covers the zero-length match prevention branch (line 38)
 * which is hard to trigger with normal patterns.
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('maskSensitiveData - zero-length match branch coverage (line 38)', () => {
  beforeEach(async () => {
    // Reset modules to clear the cached regex
    vi.resetModules();
  });

  /**
   * The zero-length match branch at line 38 is defensive code that prevents
   * infinite loops when a regex pattern matches an empty string.
   *
   * To test this, we need to:
   * 1. Create a pattern that can match zero-length strings
   * 2. Inject it into the regex system
   *
   * Since the patterns are imported from regexes.js, we mock that module.
   */
  it('should handle zero-length matches gracefully without infinite loop', async () => {
    // Mock the regexes module with a pattern that can match zero-length strings
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          // This regex can match zero-length strings (empty string before any character)
          // Using a lookbehind that always matches
          regex: /(?=a)/g,
          name: 'zero-length-test',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    // Import the mask function with mocked regexes
    const { maskSensitiveData } = await import('../src/mask.js');

    // Call with text that will trigger the zero-length match
    const input = 'aaaa';
    const result = maskSensitiveData(input);

    // Should complete without infinite loop and return input unchanged
    // (since zero-length matches don't actually mask anything meaningful)
    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  it('should handle optional pattern groups that can match empty', async () => {
    // Mock with a pattern using optional groups
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          // Pattern with optional groups: matches "SECRET=" optionally followed by content
          // When content is empty, match[0] could be just "SECRET=" which is non-empty
          regex: /SECRET=([a-z]*)?/gi,
          name: 'optional-test',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    // Test with content that has the pattern
    const input = 'SECRET=abc';
    const result = maskSensitiveData(input);

    expect(result).toBeDefined();
    expect(typeof result).toBe('string');
  });

  it('should process text with multiple potential zero-length match positions', async () => {
    // Use a lookbehind pattern that matches position after 'a'
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          // This pattern matches a position, not content
          regex: /(?<=a)(?=b)/g,
          name: 'position-test',
          matchAccuracy: 'high' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'ab ab ab';
    const result = maskSensitiveData(input);

    // Should complete without hanging
    expect(result).toBeDefined();
    // The result might be unchanged if no actual content was matched
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
          // matchAccuracy intentionally omitted to test fallback
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    const input = 'SECRET="abcdefghijklmnop"';
    const result = maskSensitiveData(input);

    // Should process without error even with missing accuracy
    expect(result).toBeDefined();
    expect(result).not.toBe(input); // Should be masked
  });
});

describe('maskSensitiveData - edge cases for match array access', () => {
  beforeEach(async () => {
    vi.resetModules();
  });

  it('should handle pattern array index bounds correctly', async () => {
    // Test with multiple patterns to verify loop iteration
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

    // Test that the third pattern is correctly identified
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
    // Create two patterns that will definitely produce overlapping matches
    // Pattern 1 matches: SECRET_VALUE (longer)
    // Pattern 2 matches: VALUE (shorter, contained within pattern 1)
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          // First pattern - matches the full SECRET_VALUE="..."
          regex: /SECRET_VALUE="[^"]+"/gi,
          name: 'full-secret',
          matchAccuracy: 'high' as const,
        },
        {
          // Second pattern - matches just VALUE="..." which overlaps
          regex: /VALUE="[^"]+"/gi,
          name: 'partial-value',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    // This input will match both patterns:
    // - Pattern 1 matches: SECRET_VALUE="1234567890" (position 7-32)
    // - Pattern 2 matches: VALUE="1234567890" (position 14-32) - overlaps!
    const input = 'PREFIX SECRET_VALUE="1234567890" SUFFIX';
    const result = maskSensitiveData(input);

    // Should be masked (first match wins, second is skipped)
    expect(result).not.toBe(input);
    expect(result.includes('*')).toBe(true);
    // The word PREFIX and SUFFIX should remain
    expect(result.startsWith('PREFIX ')).toBe(true);
    expect(result.endsWith(' SUFFIX')).toBe(true);
  });

  it('should trigger overlapping branch when inner pattern starts inside outer match', async () => {
    // This test specifically targets the else branch of: if (match.start >= lastEnd)
    vi.doMock('../src/regexes/index.js', () => ({
      allRegexPatterns: [
        {
          // Outer pattern: OUTER_INNER_END
          regex: /OUTER_INNER_END/gi,
          name: 'outer',
          matchAccuracy: 'high' as const,
        },
        {
          // Inner pattern: INNER_END (starts inside the outer match)
          regex: /INNER_END/gi,
          name: 'inner',
          matchAccuracy: 'medium' as const,
        },
      ],
    }));

    const { maskSensitiveData } = await import('../src/mask.js');

    // Both patterns match, with INNER_END starting after OUTER_INNER_END started
    // but before it ends, triggering the overlap detection
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

    // Two separate tokens - both should be masked
    const input = 'TOKEN="1234567890" TOKEN="abcdefghij"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    // Count asterisks - should have masks for both tokens
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

    // Both patterns match at the same position
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

    // Multiple matches that should all be processed
    const input = 'KEY1="1234567890" KEY2="abcdefghij" KEY3="zyxwvutsrq"';
    const result = maskSensitiveData(input);

    expect(result).not.toBe(input);
    // All three should be masked - the result should have asterisks
    expect(result.includes('*')).toBe(true);
    // The masking happens every 2 chars, so we should see a mix of * and original chars
    // Count that we have significant masking
    const asteriskCount = (result.match(/\*/g) || []).length;
    expect(asteriskCount).toBeGreaterThan(20); // Should have many masked chars
  });
});
