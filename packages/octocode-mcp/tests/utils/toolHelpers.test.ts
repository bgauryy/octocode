import { describe, it, expect, vi, afterEach } from 'vitest';
import path from 'path';

vi.mock('octocode-security-utils/pathValidator', () => {
  return {
    pathValidator: {
      validate: vi.fn((inputPath: string) => ({
        isValid: true,
        sanitizedPath: path.resolve(inputPath),
      })),
    },
  };
});

vi.mock('../../src/errors/errorFactories.js', () => ({
  ToolErrors: {
    pathValidationFailed: vi.fn(
      (p: string, msg: string) => new Error(`${p}: ${msg}`)
    ),
  },
}));

vi.mock('../../src/utils/response/error.js', () => ({
  createErrorResult: vi.fn((_err: Error, _query: unknown, _opts: unknown) => ({
    status: 'error',
    data: { error: 'mocked error' },
  })),
}));

const { validateToolPath } =
  await import('../../src/utils/file/toolHelpers.js');
const { pathValidator } = await import('octocode-security-utils/pathValidator');

afterEach(() => {
  vi.unstubAllEnvs();
  vi.mocked(pathValidator.validate).mockClear();
});

describe('validateToolPath — WORKSPACE_ROOT path resolution', () => {
  it('resolves relative path against WORKSPACE_ROOT when set', () => {
    vi.stubEnv('WORKSPACE_ROOT', '/workspace/project');

    validateToolPath({ path: 'src/index.ts' }, 'localSearchCode');

    const calledWith = vi.mocked(pathValidator.validate).mock.calls[0][0];
    expect(calledWith).toBe('/workspace/project/src/index.ts');
  });

  it('resolves relative path against process.cwd() when WORKSPACE_ROOT is not set', () => {
    delete process.env.WORKSPACE_ROOT;

    validateToolPath({ path: 'src/index.ts' }, 'localSearchCode');

    const calledWith = vi.mocked(pathValidator.validate).mock.calls[0][0];
    expect(calledWith).toBe(path.resolve(process.cwd(), 'src/index.ts'));
  });

  it('passes absolute paths through unchanged (no WORKSPACE_ROOT interference)', () => {
    vi.stubEnv('WORKSPACE_ROOT', '/workspace/project');

    validateToolPath({ path: '/absolute/path/to/file.ts' }, 'localSearchCode');

    const calledWith = vi.mocked(pathValidator.validate).mock.calls[0][0];
    expect(calledWith).toBe('/absolute/path/to/file.ts');
  });

  it('returns error result when path is empty', () => {
    const result = validateToolPath({ path: '' }, 'localSearchCode');
    expect(result.isValid).toBe(false);
  });

  it('strips file:// prefix before resolving', () => {
    vi.stubEnv('WORKSPACE_ROOT', '/workspace/project');

    validateToolPath({ path: 'file://src/index.ts' }, 'localSearchCode');

    const calledWith = vi.mocked(pathValidator.validate).mock.calls[0][0];
    expect(calledWith).toBe('/workspace/project/src/index.ts');
  });
});
