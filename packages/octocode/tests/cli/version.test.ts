import { describe, expect, it } from 'vitest';
import { formatVersion } from '../../src/cli/version.js';

describe('--version line', () => {
  it('prints one version when the launcher and native agree', () => {
    expect(formatVersion('20.0.0', '20.0.0')).toBe('octocode 20.0.0');
    expect(formatVersion('20.0.0', undefined)).toBe('octocode 20.0.0');
  });

  it('names the native version only when an install mismatches', () => {
    expect(formatVersion('20.0.0', '19.9.0')).toBe(
      'octocode 20.0.0 (native 19.9.0)'
    );
  });
});
