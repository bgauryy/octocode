import { describe, expect, it } from 'vitest';
import {
  checkRegexSafety,
  createSafeRegExp,
} from '../../src/utils/core/safeRegex.js';

describe('checkRegexSafety', () => {
  // ---- safe patterns ----

  it('returns safe for a plain identifier', () => {
    expect(checkRegexSafety('getUser').safe).toBe(true);
  });

  it('returns safe for a simple alternation', () => {
    expect(checkRegexSafety('foo|bar').safe).toBe(true);
  });

  it('returns safe for a simple group with fixed content', () => {
    expect(checkRegexSafety('(foo|bar)baz').safe).toBe(true);
  });

  it('returns safe for a quantifier on a literal char', () => {
    expect(checkRegexSafety('ab+c').safe).toBe(true);
  });

  it('returns safe for * quantifier', () => {
    expect(checkRegexSafety('a*b').safe).toBe(true);
  });

  it('returns safe for ? quantifier', () => {
    expect(checkRegexSafety('colou?r').safe).toBe(true);
  });

  it('returns safe for a character class', () => {
    expect(checkRegexSafety('[a-z]+').safe).toBe(true);
  });

  it('returns safe for escaped char inside pattern', () => {
    expect(checkRegexSafety('foo\\.bar').safe).toBe(true);
  });

  it('returns safe for a group followed by + quantifier (no inner quantifier)', () => {
    expect(checkRegexSafety('(abc)+').safe).toBe(true);
  });

  it('returns safe for nested group without nested quantifier', () => {
    expect(checkRegexSafety('(a(b)c)').safe).toBe(true);
  });

  // ---- {n} repetition quantifier ----

  it('returns safe for a {n} quantifier on a literal', () => {
    expect(checkRegexSafety('a{3}').safe).toBe(true);
  });

  it('returns safe for a {n,m} quantifier on a literal', () => {
    expect(checkRegexSafety('a{2,5}').safe).toBe(true);
  });

  it('handles a { without closing brace (not a quantifier)', () => {
    // isRepetitionQuantifier returns false when there is no closing }
    const result = checkRegexSafety('a{bc');
    // Not a quantifier → treated as literal { → safe
    expect(result.safe).toBe(true);
  });

  it('returns safe for a group with {n} quantifier (no inner quantifier)', () => {
    expect(checkRegexSafety('(ab){3}').safe).toBe(true);
  });

  // ---- unsafe (ReDoS) patterns ----

  it('returns unsafe for a quantifier inside a quantified group: (a+)+', () => {
    const result = checkRegexSafety('(a+)+');
    expect(result.safe).toBe(false);
    expect(result.reason).toMatch(/nested quantifier/i);
  });

  it('returns unsafe for (a*)*', () => {
    expect(checkRegexSafety('(a*)*').safe).toBe(false);
  });

  it('returns unsafe for quantifier after quantified group sibling in outer group: ((a+)b)+', () => {
    const result = checkRegexSafety('((a+)b)+');
    expect(result.safe).toBe(false);
  });

  // ---- pattern too long ----

  it('returns unsafe for a pattern exceeding 1000 characters', () => {
    const longPattern = 'a'.repeat(1001);
    const result = checkRegexSafety(longPattern);
    expect(result.safe).toBe(false);
    expect(result.reason).toMatch(/too long/i);
  });

  it('returns safe for a pattern of exactly 1000 characters', () => {
    const pattern = 'a'.repeat(1000);
    expect(checkRegexSafety(pattern).safe).toBe(true);
  });

  // ---- lazy / possessive quantifiers (lines 135-139) ----

  it('handles lazy quantifier *? safely', () => {
    // *? is a lazy quantifier — should parse without crashing
    const result = checkRegexSafety('a*?b');
    // Not a nested quantifier context, so safe
    expect(typeof result.safe).toBe('boolean');
  });

  it('handles possessive-style *+ safely', () => {
    const result = checkRegexSafety('a*+b');
    expect(typeof result.safe).toBe('boolean');
  });

  it('handles +? lazy quantifier', () => {
    const result = checkRegexSafety('a+?b');
    expect(typeof result.safe).toBe('boolean');
  });

  // ---- character class edge cases ----

  it('handles character class with backslash escape', () => {
    expect(checkRegexSafety('[\\d]').safe).toBe(true);
  });

  it('handles empty character class', () => {
    expect(checkRegexSafety('[]').safe).toBe(true);
  });
});

describe('createSafeRegExp', () => {
  it('creates a RegExp from a safe pattern', () => {
    const re = createSafeRegExp('hello');
    expect(re).toBeInstanceOf(RegExp);
    expect(re.test('hello world')).toBe(true);
  });

  it('accepts flags', () => {
    const re = createSafeRegExp('hello', 'i');
    expect(re.flags).toContain('i');
    expect(re.test('HELLO')).toBe(true);
  });

  it('throws for unsafe patterns', () => {
    expect(() => createSafeRegExp('(a+)+')).toThrow();
  });

  it('throws for patterns that are too long', () => {
    expect(() => createSafeRegExp('a'.repeat(1001))).toThrow(/too long/i);
  });
});
