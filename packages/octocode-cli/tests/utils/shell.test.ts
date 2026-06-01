import { describe, it, expect, vi, beforeEach } from 'vitest';
import { spawnSync } from 'node:child_process';

vi.mock('node:child_process', () => ({
  spawnSync: vi.fn(),
}));

describe('Shell Utilities', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('runCommand', () => {
    it('should return success result when command succeeds', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: 'output\n',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      const result = runCommand('echo', ['hello']);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe('output');
      expect(result.stderr).toBe('');
      expect(result.exitCode).toBe(0);
    });

    it('should return failure result when command fails', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'error message\n',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      const result = runCommand('invalid-cmd', []);

      expect(result.success).toBe(false);
      expect(result.stderr).toBe('error message');
      expect(result.exitCode).toBe(1);
    });

    it('should handle command throwing error', async () => {
      vi.mocked(spawnSync).mockImplementation(() => {
        throw new Error('Command not found');
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      const result = runCommand('nonexistent', []);

      expect(result.success).toBe(false);
      expect(result.stdout).toBe('');
      expect(result.stderr).toBe('Command not found');
      expect(result.exitCode).toBeNull();
    });

    it('should handle non-Error exception', async () => {
      vi.mocked(spawnSync).mockImplementation(() => {
        throw 'string error';
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      const result = runCommand('cmd', []);

      expect(result.success).toBe(false);
      expect(result.stderr).toBe('Unknown error');
      expect(result.exitCode).toBeNull();
    });

    it('should handle null stdout/stderr', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: null as unknown as string,
        stderr: null as unknown as string,
        pid: 123,
        output: [],
        signal: null,
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      const result = runCommand('cmd', []);

      expect(result.success).toBe(true);
      expect(result.stdout).toBe('');
      expect(result.stderr).toBe('');
    });

    it('should use correct default options', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: 'output',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      runCommand('test', ['arg1', 'arg2']);

      expect(spawnSync).toHaveBeenCalledWith('test', ['arg1', 'arg2'], {
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'pipe'],
        shell: false,
        timeout: 30000,
      });
    });

    it('should use empty args array by default', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runCommand } = await import('../../src/utils/shell.js');
      runCommand('cmd');

      expect(spawnSync).toHaveBeenCalledWith('cmd', [], expect.any(Object));
    });
  });

  describe('commandExists', () => {
    it('should return true when command exists (unix)', async () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, 'platform', { value: 'darwin' });

      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '/usr/bin/node',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { commandExists } = await import('../../src/utils/shell.js');
      const result = commandExists('node');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'which',
        ['node'],
        expect.any(Object)
      );

      Object.defineProperty(process, 'platform', { value: originalPlatform });
    });

    it('should return false when command does not exist', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'not found',
        pid: 123,
        output: [],
        signal: null,
      });

      const { commandExists } = await import('../../src/utils/shell.js');
      const result = commandExists('nonexistent-cmd');

      expect(result).toBe(false);
    });

    it('should use "where" on Windows', async () => {
      const originalPlatform = process.platform;
      Object.defineProperty(process, 'platform', { value: 'win32' });

      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: 'C:\\node.exe',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { commandExists } = await import('../../src/utils/shell.js');
      commandExists('node');

      expect(spawnSync).toHaveBeenCalledWith(
        'where',
        ['node'],
        expect.any(Object)
      );

      Object.defineProperty(process, 'platform', { value: originalPlatform });
    });
  });

  describe('getCommandVersion', () => {
    it('should return version string when command succeeds', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: 'v20.10.0\nmore info',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getCommandVersion } = await import('../../src/utils/shell.js');
      const result = getCommandVersion('node');

      expect(result).toBe('v20.10.0');
    });

    it('should return null when command fails', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: 'command not found',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getCommandVersion } = await import('../../src/utils/shell.js');
      const result = getCommandVersion('nonexistent');

      expect(result).toBeNull();
    });

    it('should use default --version flag', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '1.0.0',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getCommandVersion } = await import('../../src/utils/shell.js');
      getCommandVersion('cmd');

      expect(spawnSync).toHaveBeenCalledWith(
        'cmd',
        ['--version'],
        expect.any(Object)
      );
    });

    it('should use custom version flag', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '2.0.0',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { getCommandVersion } = await import('../../src/utils/shell.js');
      getCommandVersion('cmd', '-v');

      expect(spawnSync).toHaveBeenCalledWith('cmd', ['-v'], expect.any(Object));
    });
  });

  describe('runInteractiveCommand', () => {
    it('should return success when command succeeds', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runInteractiveCommand } =
        await import('../../src/utils/shell.js');
      const result = runInteractiveCommand('gh', ['auth', 'login']);

      expect(result.success).toBe(true);
      expect(result.exitCode).toBe(0);
    });

    it('should return failure when command fails', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 1,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runInteractiveCommand } =
        await import('../../src/utils/shell.js');
      const result = runInteractiveCommand('gh', ['auth', 'login']);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(1);
    });

    it('should use inherit stdio for interactive terminal access', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runInteractiveCommand } =
        await import('../../src/utils/shell.js');
      runInteractiveCommand('gh', ['auth', 'login']);

      expect(spawnSync).toHaveBeenCalledWith('gh', ['auth', 'login'], {
        stdio: 'inherit',
        shell: false,
      });
    });

    it('should handle exceptions gracefully', async () => {
      vi.mocked(spawnSync).mockImplementation(() => {
        throw new Error('Command failed');
      });

      const { runInteractiveCommand } =
        await import('../../src/utils/shell.js');
      const result = runInteractiveCommand('invalid', []);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBeNull();
    });

    it('should use empty args array by default', async () => {
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { runInteractiveCommand } =
        await import('../../src/utils/shell.js');
      runInteractiveCommand('cmd');

      expect(spawnSync).toHaveBeenCalledWith('cmd', [], expect.any(Object));
    });
  });
});
