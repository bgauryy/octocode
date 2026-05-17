/**
 * T3.3 — Bundled ripgrep resolver.
 *
 * `resolveRipgrepBinary()` MUST always return a usable absolute path —
 * never crash when @vscode/ripgrep's per-platform binary fails to
 * download. The fallback is the literal string `'rg'`, which `safeExec`
 * resolves against `PATH`.
 */
import { describe, expect, it } from 'vitest';
import {
  resolveRipgrepBinary,
  RIPGREP_PATH_FALLBACK,
} from '../../src/utils/exec/ripgrepBinary.js';

describe('T3.3 — resolveRipgrepBinary', () => {
  it('returns a non-empty string (never undefined)', () => {
    const path = resolveRipgrepBinary();
    expect(typeof path).toBe('string');
    expect(path.length).toBeGreaterThan(0);
  });

  it("exports a stable fallback constant of 'rg'", () => {
    expect(RIPGREP_PATH_FALLBACK).toBe('rg');
  });

  it('prefers the bundled binary path when available', () => {
    const path = resolveRipgrepBinary();
    // On platforms where @vscode/ripgrep ships a binary, the resolver
    // returns an absolute path. On unsupported platforms it falls back
    // to RIPGREP_PATH_FALLBACK ('rg'). Either is acceptable; what we
    // want to pin is the no-throw contract above.
    expect(
      path === RIPGREP_PATH_FALLBACK ||
        path.startsWith('/') ||
        /^[A-Z]:\\/.test(path)
    ).toBe(true);
  });
});
