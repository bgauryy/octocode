import { describe, expect, it } from 'vitest';
import {
  isReleasesEnabled,
  isDiscussionsEnabled,
} from '../../src/tools/toolNames.js';

describe('isReleasesEnabled', () => {
  it('returns false when ENABLE_RELEASES is not set', () => {
    expect(isReleasesEnabled({})).toBe(false);
  });

  it.each(['1', 'true', 'yes', 'on'])(
    'returns true for truthy value "%s"',
    value => {
      expect(isReleasesEnabled({ ENABLE_RELEASES: value })).toBe(true);
    }
  );

  it.each(['0', 'false', 'no', 'off', 'maybe'])(
    'returns false for falsy value "%s"',
    value => {
      expect(isReleasesEnabled({ ENABLE_RELEASES: value })).toBe(false);
    }
  );

  it('is case-insensitive', () => {
    expect(isReleasesEnabled({ ENABLE_RELEASES: 'TRUE' })).toBe(true);
    expect(isReleasesEnabled({ ENABLE_RELEASES: 'Yes' })).toBe(true);
    expect(isReleasesEnabled({ ENABLE_RELEASES: 'ON' })).toBe(true);
  });

  it('trims surrounding whitespace', () => {
    expect(isReleasesEnabled({ ENABLE_RELEASES: '  1  ' })).toBe(true);
  });
});

describe('isDiscussionsEnabled', () => {
  it('returns false when ENABLE_DISCUSSIONS is not set', () => {
    expect(isDiscussionsEnabled({})).toBe(false);
  });

  it.each(['1', 'true', 'yes', 'on'])(
    'returns true for truthy value "%s"',
    value => {
      expect(isDiscussionsEnabled({ ENABLE_DISCUSSIONS: value })).toBe(true);
    }
  );

  it('returns false for "0"', () => {
    expect(isDiscussionsEnabled({ ENABLE_DISCUSSIONS: '0' })).toBe(false);
  });
});


