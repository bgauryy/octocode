import { afterEach, describe, expect, it } from 'vitest';
import { ContentSanitizer } from '../../src/security/contentSanitizer.js';
import { securityRegistry } from '../../src/security/registry.js';

const FAKE_GH_TOKEN = 'ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';

describe('ContentSanitizer.sanitizeContent', () => {
  afterEach(() => {
    securityRegistry.reset();
  });

  it('returns empty string for null input', () => {
    // @ts-expect-error — deliberately testing runtime null coercion
    const r = ContentSanitizer.sanitizeContent(null);
    expect(r.content).toBe('');
    expect(r.hasSecrets).toBe(false);
    expect(r.secretsDetected).toHaveLength(0);
  });

  it('passes clean text through unchanged', () => {
    const clean = 'just a comment';
    const r = ContentSanitizer.sanitizeContent(clean);
    expect(r.content).toBe(clean);
    expect(r.hasSecrets).toBe(false);
  });

  it('detects and redacts a GitHub PAT', () => {
    const r = ContentSanitizer.sanitizeContent(`key=${FAKE_GH_TOKEN}`, undefined);
    expect(r.hasSecrets).toBe(true);
    expect(r.secretsDetected.length).toBeGreaterThan(0);
    expect(r.content).not.toContain(FAKE_GH_TOKEN);
    expect(r.content).toContain('[REDACTED-');
  });

  it('applies extra JS patterns from the registry', () => {
    securityRegistry.addSecretPatterns([
      { name: 'custom-key', description: 'custom-key test pattern', regex: /MYKEY-[A-Z]{8}/g },
    ]);
    const r = ContentSanitizer.sanitizeContent('config MYKEY-ABCDEFGH here');
    // Custom pattern is applied in the JS layer — secret should be detected.
    expect(r.secretsDetected).toContain('custom-key');
    expect(r.content).not.toContain('MYKEY-ABCDEFGH');
  });

  it('respects fileContext: pattern with fileContext only fires for matching paths', () => {
    securityRegistry.addSecretPatterns([
      {
        name: 'env-secret',
        description: 'env file secret test pattern',
        regex: /MYSECRET-[A-Z]+/g,
        fileContext: /\.env$/,
      },
    ]);
    // Should redact when filePath matches .env
    const r1 = ContentSanitizer.sanitizeContent('MYSECRET-HELLO', '/project/.env');
    expect(r1.secretsDetected).toContain('env-secret');

    // Should NOT redact when filePath does not match
    const r2 = ContentSanitizer.sanitizeContent('MYSECRET-HELLO', '/project/main.ts');
    expect(r2.secretsDetected).not.toContain('env-secret');
  });
});

describe('ContentSanitizer.validateInputParameters', () => {
  it('returns valid result for a clean string param', () => {
    const r = ContentSanitizer.validateInputParameters({ query: 'hello world' });
    expect(r.isValid).toBe(true);
    expect(r.hasSecrets).toBe(false);
    expect(r.sanitizedParams['query']).toBe('hello world');
  });

  it('blocks dangerous prototype-pollution keys when passed as own enumerable property', () => {
    // `{ __proto__: 'evil' }` in JS sets the *prototype*, not an own property, so
    // Object.entries never sees it. To test the actual guard, we must create an
    // object that has '__proto__' as an enumerable own property.
    const params: Record<string, unknown> = {};
    Object.defineProperty(params, '__proto__', {
      value: 'evil',
      enumerable: true,
      configurable: true,
    });
    const r = ContentSanitizer.validateInputParameters(params);
    expect(r.isValid).toBe(false);
    expect(r.warnings.some(w => /dangerous/i.test(w))).toBe(true);
  });

  it('blocks the constructor key', () => {
    const r = ContentSanitizer.validateInputParameters({ constructor: 'evil' });
    expect(r.isValid).toBe(false);
  });

  it('rejects non-object input', () => {
    // @ts-expect-error — testing runtime guard
    const r = ContentSanitizer.validateInputParameters('not an object');
    expect(r.isValid).toBe(false);
    expect(r.sanitizedParams).toEqual({});
  });

  it('detects secrets nested in a string param', () => {
    const r = ContentSanitizer.validateInputParameters({ key: `token=${FAKE_GH_TOKEN}` });
    expect(r.hasSecrets).toBe(true);
    expect(r.warnings.some(w => w.includes('key'))).toBe(true);
    // Sanitized value must not contain the raw token.
    expect(String(r.sanitizedParams['key'])).not.toContain(FAKE_GH_TOKEN);
  });

  it('truncates strings exceeding MAX_STRING_LENGTH', () => {
    const long = 'x'.repeat(10_001);
    const r = ContentSanitizer.validateInputParameters({ text: long });
    expect(String(r.sanitizedParams['text']).length).toBeLessThanOrEqual(10_000);
    expect(r.warnings.some(w => /exceeds maximum length/i.test(w))).toBe(true);
  });

  it('truncates arrays exceeding MAX_ARRAY_LENGTH', () => {
    const arr = Array.from({ length: 101 }, (_, i) => String(i));
    const r = ContentSanitizer.validateInputParameters({ items: arr });
    expect(Array.isArray(r.sanitizedParams['items'])).toBe(true);
    expect((r.sanitizedParams['items'] as unknown[]).length).toBeLessThanOrEqual(100);
    expect(r.warnings.some(w => /array exceeds maximum/i.test(w))).toBe(true);
  });

  it('rejects circular references', () => {
    const obj: Record<string, unknown> = {};
    obj['self'] = obj;
    const r = ContentSanitizer.validateInputParameters(obj);
    expect(r.isValid).toBe(false);
    expect(r.warnings.some(w => /circular/i.test(w))).toBe(true);
  });

  it('recursively validates nested objects', () => {
    const r = ContentSanitizer.validateInputParameters({
      outer: { inner: 'safe value' },
    });
    expect(r.isValid).toBe(true);
    const outer = r.sanitizedParams['outer'] as Record<string, unknown>;
    expect(outer['inner']).toBe('safe value');
  });

  it('passes non-string primitives through unchanged', () => {
    const r = ContentSanitizer.validateInputParameters({ count: 42, flag: true });
    expect(r.sanitizedParams['count']).toBe(42);
    expect(r.sanitizedParams['flag']).toBe(true);
  });
});
