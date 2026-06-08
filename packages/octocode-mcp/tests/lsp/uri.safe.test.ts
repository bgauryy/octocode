import { describe, it, expect } from 'vitest';
import { fromUriSafe, UnsafeUriError } from '../../src/lsp/uri.js';

describe('fromUriSafe — uncovered branches', () => {
  it('rejects empty string', () => {
    const result = fromUriSafe('');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('non-empty string');
  });

  it('rejects non-string (null passed as any)', () => {
    const result = fromUriSafe(null as unknown as string);
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('non-empty string');
  });

  it('rejects uri with null byte', () => {
    const result = fromUriSafe('file:///path/\u0000file.ts');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('null byte');
  });

  it('rejects uri missing a scheme', () => {
    const result = fromUriSafe('/just/a/path.ts');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('scheme');
  });

  it('rejects non-file scheme', () => {
    const result = fromUriSafe('http://example.com/file.ts');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('unsupported scheme');
  });

  it('throws UnsafeUriError when throwOnInvalid is set', () => {
    expect(() => fromUriSafe('', { throwOnInvalid: true })).toThrow(
      UnsafeUriError
    );
  });

  it('accepts a valid file URI', () => {
    const result = fromUriSafe('file:///workspace/src/index.ts');
    expect(result.isValid).toBe(true);
    expect(result.path).toContain('index.ts');
  });

  it('rejects URI whose percent-encoded path contains a null byte (line 70)', () => {
    const result = fromUriSafe('file:///path%00here');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('null byte');
  });

  it('rejects a double-slash path that causes URI.parse to throw (line 74)', () => {
    const result = fromUriSafe('file:////');
    expect(result.isValid).toBe(false);
    expect(result.error).toContain('parse failed');
  });
});
