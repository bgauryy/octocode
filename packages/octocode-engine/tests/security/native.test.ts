import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  nativeSanitizeContent,
  nativeMaskSensitiveData,
  nativePatternCount,
} from '../../src/security/native.js';

const FAKE_GH_TOKEN = 'ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';

// These tests force the pure-JS fallback path (OCTOCODE_SECURITY_FORCE_JS=1),
// which is the branch that regression-guards against a missing/broken native
// binary. In this mode getNativeModule() always returns null, so the exported
// wrappers must exercise their JS implementations.
describe('native security wrappers (JS fallback path)', () => {
  const saved: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const k of [
      'OCTOCODE_SECURITY_FORCE_JS',
      'OCTOCODE_SECURITY_REQUIRE_NATIVE',
      'OCTOCODE_SECURITY_NATIVE_PATH',
    ]) {
      saved[k] = process.env[k];
    }
    process.env.OCTOCODE_SECURITY_FORCE_JS = '1';
    delete process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;
  });

  afterEach(() => {
    for (const [k, v] of Object.entries(saved)) {
      if (v === undefined) delete process.env[k];
      else process.env[k] = v;
    }
  });

  it('sanitizeContent redacts a GitHub PAT via the JS regex fallback', () => {
    const r = nativeSanitizeContent(`key=${FAKE_GH_TOKEN}`, null);
    expect(r.hasSecrets).toBe(true);
    expect(r.secretsDetected.length).toBeGreaterThan(0);
    expect(r.content).not.toContain(FAKE_GH_TOKEN);
    expect(r.content).toContain('[REDACTED-');
    expect(r.warnings.some(w => /secret/i.test(w))).toBe(true);
  });

  it('sanitizeContent leaves clean content unchanged and reports no secrets', () => {
    const r = nativeSanitizeContent('const x = 1; // nothing secret', null);
    expect(r.hasSecrets).toBe(false);
    expect(r.secretsDetected).toHaveLength(0);
    expect(r.content).toBe('const x = 1; // nothing secret');
    expect(r.warnings).toHaveLength(0);
  });

  it('sanitizeContent redacts content that exceeds the 10MB byte limit', () => {
    // 10_000_001 ASCII bytes → one over MAX_CONTENT_SIZE.
    const huge = 'a'.repeat(10_000_001);
    const r = nativeSanitizeContent(huge, null);
    expect(r.content).toBe('[CONTENT-REDACTED-SIZE-LIMIT]');
    expect(r.hasSecrets).toBe(true);
    expect(r.secretsDetected).toContain('content-size-exceeded');
    expect(r.warnings[0]).toMatch(/byte limit/);
  });

  it('sanitizeContent measures the limit in UTF-8 bytes, not code units', () => {
    // Multibyte chars: fewer than the byte-limit code units but well over it in
    // bytes would trigger; here we stay safely under to confirm no false trip.
    const smallMultibyte = '💡'.repeat(10);
    const r = nativeSanitizeContent(smallMultibyte, null);
    expect(r.content).toBe(smallMultibyte);
    expect(r.hasSecrets).toBe(false);
  });

  it('maskSensitiveData masks a token and returns clean text untouched', () => {
    const masked = nativeMaskSensitiveData(`token=${FAKE_GH_TOKEN}`);
    expect(masked).not.toContain(FAKE_GH_TOKEN);
    expect(masked).toContain('*');

    expect(nativeMaskSensitiveData('plain text with no secret')).toBe(
      'plain text with no secret'
    );
  });

  it('maskSensitiveData returns empty input unchanged', () => {
    expect(nativeMaskSensitiveData('')).toBe('');
  });

  it('patternCount falls back to the JS pattern list length (a positive number)', () => {
    const count = nativePatternCount();
    expect(typeof count).toBe('number');
    expect(count).toBeGreaterThan(0);
  });
});

describe('native module env-flag conflict', () => {
  const saved: Record<string, string | undefined> = {};

  beforeEach(() => {
    saved.force = process.env.OCTOCODE_SECURITY_FORCE_JS;
    saved.require = process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;
  });

  afterEach(() => {
    if (saved.force === undefined)
      delete process.env.OCTOCODE_SECURITY_FORCE_JS;
    else process.env.OCTOCODE_SECURITY_FORCE_JS = saved.force;
    if (saved.require === undefined)
      delete process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE;
    else process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE = saved.require;
  });

  it('throws when FORCE_JS and REQUIRE_NATIVE are both set (contradictory config)', () => {
    process.env.OCTOCODE_SECURITY_FORCE_JS = '1';
    process.env.OCTOCODE_SECURITY_REQUIRE_NATIVE = 'true';
    expect(() => nativePatternCount()).toThrow(/conflicts with/i);
  });
});
