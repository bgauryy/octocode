import { afterEach, describe, expect, it, vi } from 'vitest';

const originalForceJs = process.env.OCTOCODE_SECURITY_FORCE_JS;
const originalRequireNative = process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;

afterEach(() => {
  if (originalForceJs === undefined) {
    delete process.env.OCTOCODE_SECURITY_FORCE_JS;
  } else {
    process.env.OCTOCODE_SECURITY_FORCE_JS = originalForceJs;
  }

  if (originalRequireNative === undefined) {
    delete process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;
  } else {
    process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE = originalRequireNative;
  }

  vi.resetModules();
});

describe('native security fallback', () => {
  it('uses the JS fallback when native loading is explicitly disabled', async () => {
    process.env.OCTOCODE_SECURITY_FORCE_JS = '1';
    delete process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;
    vi.resetModules();

    const {
      nativeMaskSensitiveData,
      nativePatternCount,
      nativeSanitizeContent,
    } = await import('../src/native.js');

    const secret = 'ghp_1234567890abcdefghijklmnopqrstuvwxyzA';
    const result = nativeSanitizeContent(`token=${secret}`, null);

    expect(nativePatternCount()).toBeGreaterThan(0);
    expect(result.hasSecrets).toBe(true);
    expect(result.content).toContain('[REDACTED-');
    expect(result.secretsDetected.length).toBeGreaterThan(0);
    expect(nativeMaskSensitiveData(secret)).not.toBe(secret);
  });

  it('throws when native is required but JS fallback is forced', async () => {
    process.env.OCTOCODE_SECURITY_FORCE_JS = '1';
    process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE = '1';
    vi.resetModules();

    const { nativePatternCount } = await import('../src/native.js');

    expect(() => nativePatternCount()).toThrow(
      /OCTOCODE_SECURITY_REQUIRE_NATIVE/
    );
  });
});
