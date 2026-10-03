import { describe, it, expect } from 'vitest';
import { getBool, getString } from '../../src/cli/options.js';

describe('getBool', () => {
  it('is true when any listed key is truthy', () => {
    expect(getBool({ json: true }, 'json')).toBe(true);
    expect(getBool({ other: true }, 'json', 'other')).toBe(true);
  });

  it('is false for absent, false, and empty-string values', () => {
    expect(getBool({}, 'json')).toBe(false);
    expect(getBool({ json: false }, 'json')).toBe(false);
    expect(getBool({ json: '' }, 'json')).toBe(false);
  });
});

describe('getString', () => {
  it('returns the first string value among the listed keys', () => {
    expect(getString({ platform: 'pi' }, 'platform')).toBe('pi');
    expect(getString({ a: true, b: 'x' }, 'a', 'b')).toBe('x');
  });

  it('returns empty string when no listed key holds a string', () => {
    expect(getString({}, 'platform')).toBe('');
    expect(getString({ platform: true }, 'platform')).toBe('');
  });
});
