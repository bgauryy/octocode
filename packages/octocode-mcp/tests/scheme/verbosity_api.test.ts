import { describe, it, expect } from 'vitest';
import * as verbosityModule from '../../src/scheme/verbosity.js';
import { isVerbose } from '../../src/scheme/verbosity.js';

describe('verbosity module — clean public API', () => {
  it('exports exactly one runtime symbol: isVerbose', () => {
    const exports = Object.keys(verbosityModule);
    expect(exports).toEqual(['isVerbose']);
  });

  it('does NOT export isConcise', () => {
    expect('isConcise' in verbosityModule).toBe(false);
  });

  it('does NOT export isCompact', () => {
    expect('isCompact' in verbosityModule).toBe(false);
  });

  it('does NOT export isBasic', () => {
    expect('isBasic' in verbosityModule).toBe(false);
  });

  it('does NOT export normalizeVerbosity', () => {
    expect('normalizeVerbosity' in verbosityModule).toBe(false);
  });

  it('does NOT export compactTrimHints', () => {
    expect('compactTrimHints' in verbosityModule).toBe(false);
  });

  it('does NOT export makeAdvisoryPredicate', () => {
    expect('makeAdvisoryPredicate' in verbosityModule).toBe(false);
  });

  it('does NOT export assertConcisePayload', () => {
    expect('assertConcisePayload' in verbosityModule).toBe(false);
  });
});

describe('isVerbose', () => {
  it('returns true only when verbose:true', () => {
    expect(isVerbose({ verbose: true })).toBe(true);
  });

  it('returns false for verbose:false', () => {
    expect(isVerbose({ verbose: false })).toBe(false);
  });

  it('returns false when verbose is omitted', () => {
    expect(isVerbose({})).toBe(false);
  });

  it('returns false when verbose is undefined', () => {
    expect(isVerbose({ verbose: undefined })).toBe(false);
  });
});
