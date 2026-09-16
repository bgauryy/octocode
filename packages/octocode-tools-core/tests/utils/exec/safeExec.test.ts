import { describe, expect, it, vi } from 'vitest';

const mockValidateCommand = vi.hoisted(() => vi.fn());
const mockValidateArgs = vi.hoisted(() => vi.fn());
const mockSpawnWithTimeout = vi.hoisted(() => vi.fn());

vi.mock('@octocodeai/octocode-engine/commandValidator', () => ({
  validateCommand: mockValidateCommand,
}));

vi.mock('../../../src/utils/exec/spawn/wrappers.js', () => ({
  spawnWithTimeout: mockSpawnWithTimeout,
  validateArgs: mockValidateArgs,
}));

const { safeExec } = await import('../../../src/utils/exec/safe.js');

const okSpawn = {
  stdout: 'output',
  stderr: '',
  exitCode: 0,
  success: true,
};

describe('safeExec', () => {
  it('returns stdout on success', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: true });
    mockSpawnWithTimeout.mockResolvedValue(okSpawn);

    const result = await safeExec('git', ['status']);
    expect(result.success).toBe(true);
    expect(result.stdout).toBe('output');
    expect(result.code).toBe(0);
  });

  it('throws when command validation fails', async () => {
    mockValidateCommand.mockReturnValue({
      isValid: false,
      error: 'not allowed',
    });

    await expect(safeExec('rm', ['-rf', '/'])).rejects.toThrow(
      'Command validation failed: not allowed'
    );
    expect(mockSpawnWithTimeout).not.toHaveBeenCalled();
  });

  it('throws generic message when command validation has no error text', async () => {
    mockValidateCommand.mockReturnValue({ isValid: false });

    await expect(safeExec('evil', [])).rejects.toThrow(
      'Command not allowed'
    );
  });

  it('throws when arg validation fails', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: false, error: 'bad arg' });

    await expect(safeExec('git', ['--bad'])).rejects.toThrow(
      'Argument validation failed: bad arg'
    );
    expect(mockSpawnWithTimeout).not.toHaveBeenCalled();
  });

  it('throws generic message when arg validation has no error text', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: false });

    await expect(safeExec('git', ['--bad'])).rejects.toThrow(
      'Invalid arguments'
    );
  });

  it('throws when spawn returns an error', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: true });
    mockSpawnWithTimeout.mockResolvedValue({
      ...okSpawn,
      success: false,
      error: new Error('spawn failed'),
    });

    await expect(safeExec('git', ['log'])).rejects.toThrow('spawn failed');
  });

  it('returns truncated result when outputLimitExceeded and tolerateOutputLimit', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: true });
    mockSpawnWithTimeout.mockResolvedValue({
      stdout: 'partial output',
      stderr: '',
      exitCode: 0,
      success: true,
      outputLimitExceeded: true,
    });

    const result = await safeExec('cat', ['huge.bin'], {
      tolerateOutputLimit: true,
    });
    expect(result.truncated).toBe(true);
    expect(result.stdout).toBe('partial output');
    expect(result.success).toBe(true);
  });

  it('passes timeout, cwd and env to spawnWithTimeout', async () => {
    mockValidateCommand.mockReturnValue({ isValid: true });
    mockValidateArgs.mockReturnValue({ valid: true });
    mockSpawnWithTimeout.mockResolvedValue(okSpawn);

    await safeExec('git', ['status'], {
      timeout: 5000,
      cwd: '/repo',
      env: { GIT_DIR: '.git' },
    });

    expect(mockSpawnWithTimeout).toHaveBeenCalledWith(
      'git',
      ['status'],
      expect.objectContaining({ timeout: 5000, cwd: '/repo' })
    );
  });
});
