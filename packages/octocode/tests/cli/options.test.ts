import { describe, it, expect } from 'vitest';
import { getBool, getString } from '../../src/cli/options.js';

describe('getBool', () => {
  it('is true when the key is truthy', () => {
    expect(getBool({ json: true }, 'json')).toBe(true);
    expect(getBool({ json: 'yes' }, 'json')).toBe(true);
  });

  it('is false for absent, false, and empty-string values', () => {
    expect(getBool({}, 'json')).toBe(false);
    expect(getBool({ json: false }, 'json')).toBe(false);
    expect(getBool({ json: '' }, 'json')).toBe(false);
  });
});

describe('getString', () => {
  it('returns the string value of the key', () => {
    expect(getString({ platform: 'pi' }, 'platform')).toBe('pi');
  });

  it('returns empty string when the key holds no string', () => {
    expect(getString({}, 'platform')).toBe('');
    expect(getString({ platform: true }, 'platform')).toBe('');
  });
});
