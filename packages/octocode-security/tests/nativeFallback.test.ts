import { afterEach, describe, expect, it, vi } from 'vitest';

describe('native loader JS fallback', () => {
  const originalForceJs = process.env.OCTOCODE_SECURITY_FORCE_JS;

  afterEach(() => {
    if (originalForceJs === undefined) {
      delete process.env.OCTOCODE_SECURITY_FORCE_JS;
    } else {
      process.env.OCTOCODE_SECURITY_FORCE_JS = originalForceJs;
    }
    vi.resetModules();
  });

  it('detects and masks secrets when native loading is bypassed', async () => {
    vi.resetModules();
    process.env.OCTOCODE_SECURITY_FORCE_JS = '1';

    const native = await import('../src/native.js');
    const token = 'ghp_1234567890abcdefghijklmnopqrstuvwxyz123456';
    const awsKey = 'AKIAIOSFODNN7EXAMPLE';

    expect(native.nativePatternCount()).toBe(309);

    const sanitized = native.nativeSanitizeContent(`token: ${token}`, null);
    expect(sanitized.hasSecrets).toBe(true);
    expect(sanitized.content).not.toContain(token);

    const masked = native.nativeMaskSensitiveData(`key: ${awsKey}`);
    expect(masked).not.toContain(awsKey);
  });
});
