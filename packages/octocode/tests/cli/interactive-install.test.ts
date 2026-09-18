import { describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  spawn: vi.fn(),
  select: vi.fn(),
  delegate: vi.fn(),
}));
vi.mock('node:child_process', () => ({ spawnSync: mocks.spawn }));
vi.mock('../../src/utils/prompts.js', () => ({ select: mocks.select }));
vi.mock('../../src/cli/native-delegate.js', () => ({
  delegateToNative: mocks.delegate,
}));

describe('interactive native install picker', () => {
  it('discovers clients from native and delegates the selected id', async () => {
    mocks.spawn.mockReturnValue({
      status: 0,
      stdout: '{"supported":["cursor","zed"]}',
      stderr: '',
    });
    mocks.select.mockResolvedValue('zed');
    mocks.delegate.mockReturnValue(0);
    const { runInteractiveInstall } =
      await import('../../src/cli/interactive-install.js');
    await expect(
      runInteractiveInstall('/bin/octocode', ['install'])
    ).resolves.toBe(0);
    expect(mocks.spawn).toHaveBeenCalledWith(
      '/bin/octocode',
      ['install', '--list', '--json'],
      expect.objectContaining({ encoding: 'utf8' })
    );
    expect(mocks.delegate).toHaveBeenCalledWith('/bin/octocode', [
      'install',
      '--ide',
      'zed',
    ]);
  });

  it('rejects malformed native discovery output', async () => {
    mocks.spawn.mockReturnValue({
      status: 0,
      stdout: '{"supported":[1]}',
      stderr: '',
    });
    const { runInteractiveInstall } =
      await import('../../src/cli/interactive-install.js');
    await expect(
      runInteractiveInstall('/bin/octocode', ['install'])
    ).rejects.toThrow('invalid response');
  });
});
