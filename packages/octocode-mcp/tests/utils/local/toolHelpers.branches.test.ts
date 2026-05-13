/**
 * Branch coverage tests for toolHelpers.ts getPathErrorHints
 * Targets uncovered branches: Permission denied, Symlink, ENOENT/not found
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn(),
  },
}));

vi.mock('../../../src/hints/index.js', () => ({
  getHints: vi.fn(() => []),
}));

import { pathValidator } from 'octocode-security-utils/pathValidator';
import { validateToolPath } from '../../../src/utils/file/toolHelpers.js';

describe('getPathErrorHints - branch coverage', () => {
  const baseQuery = {
    path: '/workspace/some/file.ts',
    researchGoal: 'test',
    reasoning: 'test',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should provide Permission denied hint when error contains "Permission denied"', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'Permission denied for path',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FETCH_CONTENT');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(
      hints.some(h => h.includes('Check file/directory permissions'))
    ).toBe(true);
  });

  it('should provide Symlink hint when error contains "Symlink"', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'Symlink resolution failed for path',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FETCH_CONTENT');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(hints.some(h => h.includes('Symlink target may be outside'))).toBe(
      true
    );
  });

  it('should provide symlink hint (lowercase) when error contains "symlink"', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'Could not resolve symlink path',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FETCH_CONTENT');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(hints.some(h => h.includes('Symlink target may be outside'))).toBe(
      true
    );
  });

  it('should provide ENOENT hint when error contains "ENOENT"', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'ENOENT: no such file or directory',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FIND_FILES');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(hints.some(h => h.includes('Path not found'))).toBe(true);
  });

  it('should provide not found hint when error contains "not found"', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'File not found at the specified location',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FIND_FILES');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(hints.some(h => h.includes('Path not found'))).toBe(true);
  });

  it('should provide "outside allowed" hint for that error type', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: false,
      error: 'Path is outside allowed directories',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FIND_FILES');

    expect(result.isValid).toBe(false);
    const hints = result.errorResult?.hints as string[];
    expect(
      hints.some(h => h.includes('Use absolute path within workspace'))
    ).toBe(true);
  });

  it('should return valid result when validation passes', () => {
    vi.mocked(pathValidator.validate).mockReturnValue({
      isValid: true,
      sanitizedPath: '/workspace/some/file.ts',
    });

    const result = validateToolPath(baseQuery, 'LOCAL_FIND_FILES');

    expect(result.isValid).toBe(true);
    expect(result.sanitizedPath).toBe('/workspace/some/file.ts');
  });
});
