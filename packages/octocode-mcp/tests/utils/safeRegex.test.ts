import { describe, it, expect } from 'vitest';
import {
  checkRegexSafety,
  createSafeRegExp,
} from '../../src/utils/core/safeRegex.js';

describe('safeRegex', () => {
  describe('checkRegexSafety', () => {
    it('should accept simple patterns', () => {
      expect(checkRegexSafety('abc').safe).toBe(true);
      expect(checkRegexSafety('a+b').safe).toBe(true);
      expect(checkRegexSafety('\\d+').safe).toBe(true);
      expect(checkRegexSafety('foo|bar').safe).toBe(true);
      expect(checkRegexSafety('[a-z]+').safe).toBe(true);
      expect(checkRegexSafety('^export.*function').safe).toBe(true);
    });

    it('should accept patterns with quantifiers at top level', () => {
      expect(checkRegexSafety('a+').safe).toBe(true);
      expect(checkRegexSafety('a*').safe).toBe(true);
      expect(checkRegexSafety('a{2,5}').safe).toBe(true);
    });

    it('should accept simple groups with quantifiers', () => {
      expect(checkRegexSafety('(abc)+').safe).toBe(true);
      expect(checkRegexSafety('(a|b)+').safe).toBe(true);
      expect(checkRegexSafety('(\\d{2,4})-').safe).toBe(true);
    });

    it('should reject nested quantifiers (star height > 1)', () => {
      expect(checkRegexSafety('(a+)+').safe).toBe(false);
      expect(checkRegexSafety('(a*)*').safe).toBe(false);
      expect(checkRegexSafety('(a+)*').safe).toBe(false);
      expect(checkRegexSafety('(.*a)+').safe).toBe(false);
    });

    it('should reject nested quantifiers in deeper groups', () => {
      expect(checkRegexSafety('((a+))+').safe).toBe(false);
    });

    it('should reject patterns that are too long', () => {
      const longPattern = 'a'.repeat(1001);
      const result = checkRegexSafety(longPattern);
      expect(result.safe).toBe(false);
      expect(result.reason).toContain('too long');
    });

    it('should accept patterns with quantifiers inside character classes', () => {
      expect(checkRegexSafety('[+*?]+').safe).toBe(true);
    });

    it('should handle escaped characters', () => {
      expect(checkRegexSafety('\\(a+\\)+').safe).toBe(true);
    });
  });

  describe('createSafeRegExp', () => {
    it('should create RegExp for safe patterns', () => {
      const re = createSafeRegExp('abc', 'i');
      expect(re).toBeInstanceOf(RegExp);
      expect(re.test('ABC')).toBe(true);
    });

    it('should throw for unsafe patterns', () => {
      expect(() => createSafeRegExp('(a+)+')).toThrow('Nested quantifiers');
    });

    it('should throw for invalid regex syntax', () => {
      expect(() => createSafeRegExp('(?P<name>')).toThrow();
    });
  });
});
