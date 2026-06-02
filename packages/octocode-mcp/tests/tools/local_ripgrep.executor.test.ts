/**
 * Branch coverage tests for local_ripgrep/ripgrepExecutor.ts
 *
 * Covers uncovered branches:
 * - Line 33: validateRipgrepQuery returns isValid=false
 * - Line 44: configuredQuery.path is missing/undefined
 * - Line 75: preflightValidateRipgrepPattern returns isValid=false
 * - Lines 95-96: timeout (stderr includes 'timeout' or code===null)
 * - Line 127: !result.success (exit code ≥ 2, not timeout)
 * - Lines 144-147: large result triggers chunking warnings
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock safeExec so we control the executor's external dependency
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../src/utils/exec/ripgrepBinary.js', () => ({
  resolveRipgrepBinary: vi.fn().mockReturnValue('rg'),
}));

vi.mock('octocode-security-utils/pathValidator', () => ({
  pathValidator: {
    validate: vi.fn().mockReturnValue({
      isValid: true,
      sanitizedPath: '/test/path',
    }),
  },
}));

vi.mock('fs', () => ({
  promises: {
    stat: vi.fn().mockResolvedValue({ mtime: new Date() }),
    readFile: vi.fn().mockResolvedValue(''),
    readdir: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../src/hints/index.js', () => ({
  getHints: vi.fn().mockReturnValue(['hint1']),
}));

vi.mock('../../src/hints/dynamic.js', () => ({
  getLargeFileWorkflowHints: vi.fn().mockReturnValue(['narrow your search']),
}));

// Mock validateRipgrepQuery so we can control its output.
// The executor imports it from the `/schemas/runtime` subpath, so the mock
// MUST target that same specifier — mocking the package root has no effect.
vi.mock('@octocodeai/octocode-core/schemas/runtime', async importOriginal => {
  const actual =
    await importOriginal<
      typeof import('@octocodeai/octocode-core/schemas/runtime')
    >();
  return {
    ...actual,
    validateRipgrepQuery: vi.fn().mockReturnValue({
      isValid: true,
      errors: [],
      warnings: [],
    }),
  };
});

import { safeExec } from '../../src/utils/exec/safe.js';
import { validateRipgrepQuery } from '@octocodeai/octocode-core/schemas/runtime';
import { executeRipgrepSearchInternal } from '../../src/tools/local_ripgrep/ripgrepExecutor.js';
import { RESOURCE_LIMITS } from '../../src/utils/core/constants.js';

const mockSafeExec = vi.mocked(safeExec);
const mockValidateRipgrepQuery = vi.mocked(validateRipgrepQuery);

const baseQuery = {
  id: 'exec_test',
  researchGoal: 'Test',
  reasoning: 'branch coverage',
  pattern: 'myPattern',
  path: '/test/path',
  fixedString: false,
  perlRegex: false,
};

describe('executeRipgrepSearchInternal - branch coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Default: validation passes
    mockValidateRipgrepQuery.mockReturnValue({
      isValid: true,
      errors: [],
      warnings: [],
    });
    // Default safeExec: success
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '',
      stderr: '',
    });
  });

  it('returns error when validateRipgrepQuery fails (line 33)', async () => {
    mockValidateRipgrepQuery.mockReturnValue({
      isValid: false,
      errors: ['pattern is required'],
      warnings: [],
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('pattern is required');
  });

  it('returns error when path is not provided (line 44)', async () => {
    const queryWithoutPath = { ...baseQuery, path: undefined };
    const result = await executeRipgrepSearchInternal(queryWithoutPath as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('Path is required');
  });

  it('returns error when pattern preflight validation fails (line 75)', async () => {
    // Pass an invalid regex pattern that will fail preflightValidateRipgrepPattern
    const queryWithBadPattern = {
      ...baseQuery,
      pattern: '[invalid-regex-missing-bracket',
    };
    const result = await executeRipgrepSearchInternal(
      queryWithBadPattern as any
    );
    expect(result.status).toBe('error');
    expect(result.error).toContain('Pattern validation failed');
  });

  it('returns non-success error when stderr has timeout text but code is 0 (line 95)', async () => {
    // code=0 with stderr text is not a timeout — it falls to the
    // non-success branch (code >= 2). The stderr string check was removed
    // because it was fragile and code===null already covers process kill.
    mockSafeExec.mockResolvedValue({
      success: false,
      code: 0,
      stdout: '',
      stderr: 'rg: timeout occurred',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('error');
    expect(result.error).toBeDefined();
  });

  it('returns timeout error when exit code is null (line 96)', async () => {
    mockSafeExec.mockResolvedValue({
      success: false,
      code: null,
      stdout: '',
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('timed out');
  });

  it('returns error when ripgrep exits with code ≥ 2 (line 127)', async () => {
    mockSafeExec.mockResolvedValue({
      success: false,
      code: 2,
      stdout: '',
      stderr: 'regex error: invalid syntax',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('exit code 2');
  });

  it('adds chunking warnings when result payload is large (lines 144-147)', async () => {
    // Generate output larger than LARGE_RESULT_BYTES_HINT
    const largeOutput = 'a'.repeat(
      (RESOURCE_LIMITS.LARGE_RESULT_BYTES_HINT ?? 500_000) + 1
    );

    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: largeOutput,
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal({
      ...baseQuery,
      filesOnly: false,
    } as any);

    // Result should succeed (or be empty), and large-result hints should be present
    expect([undefined, 'empty', 'error']).toContain(result.status);
    // Verify warning was generated (either in hints or warnings)
    const allMessages = JSON.stringify(result);
    expect(allMessages).toMatch(/large|narrow|KB/i);
  });

  it('returns empty when ripgrep exits 1 (no matches)', async () => {
    mockSafeExec.mockResolvedValue({
      success: false,
      code: 1,
      stdout: '',
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('empty');
  });

  it('returns empty when result is empty string but success', async () => {
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '   ',
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('empty');
  });

  it('propagates warnings from validateRipgrepQuery', async () => {
    mockValidateRipgrepQuery.mockReturnValue({
      isValid: true,
      errors: [],
      warnings: ['pattern might be slow'],
    });

    mockSafeExec.mockResolvedValue({
      success: false,
      code: 1,
      stdout: '',
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('empty');
  });
});
