import { describe, expect, it } from 'vitest';
import {
  isDiscussionsEnabled,
  isReleasesEnabled,
} from '../../src/tools/toolNames.js';

describe('optional GitHub tool flags', () => {
  it.each([
    ['releases', isReleasesEnabled, 'ENABLE_RELEASES'],
    ['discussions', isDiscussionsEnabled, 'ENABLE_DISCUSSIONS'],
  ] as const)(
    '%s defaults off and accepts canonical booleans',
    (_, check, key) => {
      expect(check({})).toBe(false);
      expect(check({ [key]: '1' })).toBe(true);
      expect(check({ [key]: 'true' })).toBe(true);
      expect(check({ [key]: 'TRUE' })).toBe(true);
      expect(check({ [key]: '0' })).toBe(false);
      expect(check({ [key]: 'false' })).toBe(false);
      expect(check({ [key]: 'yes' })).toBe(false);
    }
  );
});
