import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { execFile } from 'child_process';
import { getGhCliToken } from '../../src/credentials/ghCli.js';

vi.mock('child_process', () => ({
  execFile: vi.fn(),
}));

const MOCK_PATH_WITHOUT_COMMON = '/usr/bin:/bin';
const MOCK_PATH_WITH_ALL_COMMON =
  '/opt/homebrew/bin:/usr/local/bin:/home/linuxbrew/.linuxbrew/bin:/usr/bin:/bin';

describe('getGhCliToken', () => {
  const originalPath = process.env.PATH;

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (originalPath === undefined) {
      delete process.env.PATH;
    } else {
      process.env.PATH = originalPath;
    }
  });

  it('returns trimmed token and augments PATH when common gh paths are missing', async () => {
    process.env.PATH = MOCK_PATH_WITHOUT_COMMON;

    vi.mocked(execFile).mockImplementation((file, args, options, callback) => {
      expect(file).toBe('gh');
      expect(args).toEqual(['auth', 'token']);
      expect((options as { timeout: number }).timeout).toBe(5000);
      expect((options as { env: { PATH: string } }).env.PATH).toContain(
        '/opt/homebrew/bin'
      );
      (callback as (err: Error | null, stdout: string) => void)(
        null,
        '  ghp_token_value  \n'
      );
      return {} as never;
    });

    await expect(getGhCliToken()).resolves.toBe('ghp_token_value');
  });

  it('returns null when gh returns an error', async () => {
    process.env.PATH = MOCK_PATH_WITHOUT_COMMON;

    vi.mocked(execFile).mockImplementation((_, __, ___, callback) => {
      (callback as (err: Error, stdout: string) => void)(
        new Error('gh failed'),
        ''
      );
      return {} as never;
    });

    await expect(getGhCliToken()).resolves.toBeNull();
  });

  it('returns null when stdout is empty', async () => {
    process.env.PATH = MOCK_PATH_WITHOUT_COMMON;

    vi.mocked(execFile).mockImplementation((_, __, ___, callback) => {
      (callback as (err: null, stdout: string) => void)(null, '');
      return {} as never;
    });

    await expect(getGhCliToken()).resolves.toBeNull();
  });

  it('passes hostname argument and does not duplicate PATH entries', async () => {
    process.env.PATH = MOCK_PATH_WITH_ALL_COMMON;

    vi.mocked(execFile).mockImplementation((file, args, options, callback) => {
      expect(file).toBe('gh');
      expect(args).toEqual(['auth', 'token', '--hostname', 'github.example']);
      expect((options as { env: { PATH: string } }).env.PATH).toBe(
        MOCK_PATH_WITH_ALL_COMMON
      );
      (callback as (err: null, stdout: string) => void)(null, 'tok');
      return {} as never;
    });

    await expect(getGhCliToken('github.example')).resolves.toBe('tok');
  });
});
