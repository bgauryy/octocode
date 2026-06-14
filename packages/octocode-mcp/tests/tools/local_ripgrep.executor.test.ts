import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../../octocode-tools-core/src/utils/exec/safe.js', () => ({
  safeExec: vi.fn(),
}));

vi.mock('../../../octocode-tools-core/src/utils/exec/ripgrepBinary.js', () => ({
  resolveRipgrepBinary: vi.fn().mockReturnValue('rg'),
}));

vi.mock('octocode-security/pathValidator', () => ({
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

vi.mock('../../../octocode-tools-core/src/hints/index.js', () => ({
  getHints: vi.fn().mockReturnValue(['hint1']),
}));

vi.mock('../../../octocode-tools-core/src/hints/dynamic.js', () => ({
  getLargeFileWorkflowHints: vi.fn().mockReturnValue(['narrow your search']),
}));

import { safeExec } from '../../../octocode-tools-core/src/utils/exec/safe.js';
import { executeRipgrepSearchInternal } from '../../../octocode-tools-core/src/tools/local_ripgrep/ripgrepExecutor.js';
import { RESOURCE_LIMITS } from '../../../octocode-tools-core/src/utils/core/constants.js';

const mockSafeExec = vi.mocked(safeExec);

const baseQuery = {
  id: 'exec_test',
  researchGoal: 'Test',
  reasoning: 'branch coverage',
  keywords: 'myPattern',
  path: '/test/path',
  fixedString: false,
  perlRegex: false,
};

describe('executeRipgrepSearchInternal - branch coverage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSafeExec.mockResolvedValue({
      success: true,
      code: 0,
      stdout: '',
      stderr: '',
    });
  });

  it('returns error when local schema validation fails', async () => {
    const result = await executeRipgrepSearchInternal({
      ...baseQuery,
      keywords: undefined,
    } as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('keywords');
  });

  it('returns error when path is not provided (line 44)', async () => {
    const queryWithoutPath = { ...baseQuery, path: undefined };
    const result = await executeRipgrepSearchInternal(queryWithoutPath as any);
    expect(result.status).toBe('error');
    expect(result.error).toContain('Query validation failed');
  });

  it('returns error when pattern preflight validation fails (line 75)', async () => {
    const queryWithBadPattern = {
      ...baseQuery,
      keywords: '[invalid-regex-missing-bracket',
    };
    const result = await executeRipgrepSearchInternal(
      queryWithBadPattern as any
    );
    expect(result.status).toBe('error');
    expect(result.error).toContain('Pattern validation failed');
  });

  it('returns non-success error when stderr has timeout text but code is 0 (line 95)', async () => {
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

    expect([undefined, 'empty', 'error']).toContain(result.status);
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

  it('returns empty without runtime validation warnings', async () => {
    mockSafeExec.mockResolvedValue({
      success: false,
      code: 1,
      stdout: '',
      stderr: '',
    });

    const result = await executeRipgrepSearchInternal(baseQuery as any);
    expect(result.status).toBe('empty');
    expect(result.warnings).toEqual([]);
  });
});
