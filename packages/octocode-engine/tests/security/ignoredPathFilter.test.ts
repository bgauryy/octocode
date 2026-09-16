import { describe, expect, it } from 'vitest';

import { shouldIgnorePath } from '../../src/security/ignoredPathFilter.js';

describe('ignoredPathFilter', () => {
  it('does not treat macOS /private/tmp as a sensitive private directory', () => {
    expect(shouldIgnorePath('/private/tmp/octocode-audit/archive.zip')).toBe(
      false
    );
  });

  it('still blocks user private directories', () => {
    expect(shouldIgnorePath('/Users/tester/private/archive.zip')).toBe(true);
  });
});
