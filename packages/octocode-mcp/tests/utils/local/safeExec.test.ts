import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ChildProcess, spawn } from 'child_process';
import { EventEmitter } from 'events';

class MockChildProcess extends EventEmitter {
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  pid = 12345;
  killed = false;
  exitCode: number | null = null;
  signalCode: string | null = null;

  kill(signal?: string): boolean {
    this.killed = true;
    this.signalCode = signal || 'SIGTERM';
    return true;
  }

  simulateSuccess(stdout = '', stderr = ''): void {
    setTimeout(() => {
      if (stdout) this.stdout.emit('data', Buffer.from(stdout));
      if (stderr) this.stderr.emit('data', Buffer.from(stderr));
      this.exitCode = 0;
      this.emit('close', 0);
    }, 10);
  }

  simulateFailure(exitCode = 1, stderr = '', stdout = ''): void {
    setTimeout(() => {
      if (stdout) this.stdout.emit('data', Buffer.from(stdout));
      if (stderr) this.stderr.emit('data', Buffer.from(stderr));
      this.exitCode = exitCode;
      this.emit('close', exitCode);
    }, 10);
  }

  simulateError(error: Error): void {
    setTimeout(() => {
      this.emit('error', error);
    }, 10);
  }

  simulateTimeout(): void {
    setTimeout(() => {
      this.stdout.emit('data', Buffer.from('some output'));
    }, 10);
  }
}

describe('safeExec', () => {
  let mockProcess: MockChildProcess;
  let originalWorkspaceRoot: string | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);
    originalWorkspaceRoot = process.env.WORKSPACE_ROOT;
    process.env.WORKSPACE_ROOT = process.cwd();
  });

  afterEach(() => {
    vi.clearAllTimers();
    if (originalWorkspaceRoot !== undefined) {
      process.env.WORKSPACE_ROOT = originalWorkspaceRoot;
    } else {
      delete process.env.WORKSPACE_ROOT;
    }
  });

  describe('stderr handling', () => {
    it('should collect stderr output', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], { cwd: process.cwd() });

      setTimeout(() => {
        mockProcess.stdout.emit('data', Buffer.from('stdout content'));
        mockProcess.stderr.emit('data', Buffer.from('stderr content'));
        mockProcess.emit('close', 0);
      }, 10);

      const result = await promise;

      expect(result.stdout).toBe('stdout content');
      expect(result.stderr).toBe('stderr content');
      expect(result.success).toBe(true);
    });

    it('should accumulate stderr from multiple data events', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], { cwd: process.cwd() });

      setTimeout(() => {
        mockProcess.stderr.emit('data', Buffer.from('error part 1, '));
        mockProcess.stderr.emit('data', Buffer.from('error part 2, '));
        mockProcess.stderr.emit('data', Buffer.from('error part 3'));
        mockProcess.emit('close', 0);
      }, 10);

      const result = await promise;

      expect(result.stderr).toBe('error part 1, error part 2, error part 3');
    });

    it('should ignore stderr data after process is killed', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 100,
      });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(150);

      mockProcess.stderr.emit('data', Buffer.from('late stderr'));

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout');

      vi.useRealTimers();
    });
  });

  describe('output size limit via stdout', () => {
    it('should reject when stdout exceeds maxOutputSize', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        const largeData = 'x'.repeat(maxSize + 50);
        mockProcess.stdout.emit('data', Buffer.from(largeData));
      }, 10);

      await expect(promise).rejects.toThrow('Output size limit exceeded');
      expect(mockProcess.killed).toBe(true);
    });

    it('should handle multiple stdout chunks that cumulatively exceed limit', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        mockProcess.stdout.emit('data', Buffer.from('x'.repeat(40)));
        mockProcess.stdout.emit('data', Buffer.from('y'.repeat(40)));
        mockProcess.stdout.emit('data', Buffer.from('z'.repeat(40)));
      }, 10);

      await expect(promise).rejects.toThrow('Output size limit exceeded');
      expect(mockProcess.killed).toBe(true);
    });

    it('should ignore stdout data after process is killed', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 100,
      });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(150);

      mockProcess.stdout.emit('data', Buffer.from('late stdout'));

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout');

      vi.useRealTimers();
    });
  });

  describe('output size limit via stderr', () => {
    it('should reject when stderr exceeds maxOutputSize', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        const largeData = 'x'.repeat(maxSize + 50);
        mockProcess.stderr.emit('data', Buffer.from(largeData));
      }, 10);

      await expect(promise).rejects.toThrow('Output size limit exceeded');
      expect(mockProcess.killed).toBe(true);
    });

    it('should reject when combined stdout and stderr exceeds maxOutputSize', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        mockProcess.stdout.emit('data', Buffer.from('x'.repeat(60)));
        mockProcess.stderr.emit('data', Buffer.from('y'.repeat(50)));
      }, 10);

      await expect(promise).rejects.toThrow('Output size limit exceeded');
      expect(mockProcess.killed).toBe(true);
    });

    it('should not reject when stderr is under maxOutputSize', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        mockProcess.stderr.emit('data', Buffer.from('small stderr'));
        mockProcess.emit('close', 0);
      }, 10);

      const result = await promise;

      expect(result.stderr).toBe('small stderr');
      expect(result.success).toBe(true);
    });

    it('should handle multiple stderr chunks that cumulatively exceed limit', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 100;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        maxOutputSize: maxSize,
      });

      setTimeout(() => {
        mockProcess.stderr.emit('data', Buffer.from('x'.repeat(40)));
        mockProcess.stderr.emit('data', Buffer.from('y'.repeat(40)));
        mockProcess.stderr.emit('data', Buffer.from('z'.repeat(40)));
      }, 10);

      await expect(promise).rejects.toThrow('Output size limit exceeded');
      expect(mockProcess.killed).toBe(true);
    });
  });

  describe('spawn error handling', () => {
    it('should reject on spawn error event', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], { cwd: process.cwd() });

      mockProcess.simulateError(new Error('ENOENT: command not found'));

      await expect(promise).rejects.toThrow('ENOENT: command not found');
    });

    it('should ignore error event after process is already killed', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 100,
      });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(150);

      mockProcess.emit('error', new Error('late error'));

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout');

      vi.useRealTimers();
    });

    it('should handle spawn failure during process creation', async () => {
      vi.mocked(spawn).mockImplementation(() => {
        throw new Error('Failed to spawn process');
      });

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      await expect(safeExec('ls', [], { cwd: process.cwd() })).rejects.toThrow(
        'Failed to spawn process'
      );
    });

    it('should handle non-Error spawn failures', async () => {
      vi.mocked(spawn).mockImplementation(() => {
        throw 'some string error';
      });

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      await expect(safeExec('ls', [], { cwd: process.cwd() })).rejects.toThrow(
        "Failed to spawn command 'ls'"
      );
    });
  });

  describe('timeout handling', () => {
    it('should reject on timeout', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 500,
      });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(600);

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout after 500ms');
      expect(mockProcess.killed).toBe(true);

      vi.useRealTimers();
    });

    it('should use default timeout of 30 seconds', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], { cwd: process.cwd() });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(31000);

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout after 30000ms');
      expect(mockProcess.killed).toBe(true);

      vi.useRealTimers();
    });

    it('should not reject twice when timeout fires after output size exceeded', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const maxSize = 50;
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 500,
        maxOutputSize: maxSize,
      });

      mockProcess.stdout.emit('data', Buffer.from('x'.repeat(maxSize + 50)));

      await expect(promise).rejects.toThrow('Output size limit exceeded');

      await vi.advanceTimersByTimeAsync(600);

      vi.useRealTimers();
    });
  });

  describe('successful execution', () => {
    it('should resolve with success on exit code 0', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], { cwd: process.cwd() });

      mockProcess.simulateSuccess('file1\nfile2\n', '');

      const result = await promise;

      expect(result.success).toBe(true);
      expect(result.code).toBe(0);
      expect(result.stdout).toBe('file1\nfile2\n');
    });

    it('should resolve with failure on non-zero exit code', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['nonexistent'], { cwd: process.cwd() });

      mockProcess.simulateFailure(1, 'No such file or directory');

      const result = await promise;

      expect(result.success).toBe(false);
      expect(result.code).toBe(1);
      expect(result.stderr).toBe('No such file or directory');
    });
  });

  describe('validation', () => {
    it('should reject invalid command', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      await expect(
        safeExec('rm', ['-rf', '/'], { cwd: process.cwd() })
      ).rejects.toThrow('Command validation failed');
    });

    it('should handle command validation failure without error message', async () => {
      const commandValidatorModule =
        await import('octocode-security/commandValidator');
      const validateCommandSpy = vi.spyOn(
        commandValidatorModule,
        'validateCommand'
      );
      validateCommandSpy.mockReturnValueOnce({ isValid: false });

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      await expect(safeExec('ls', [], { cwd: process.cwd() })).rejects.toThrow(
        'Command validation failed: Command not allowed'
      );

      validateCommandSpy.mockRestore();
    });

    it('should reject arguments containing null bytes', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      await expect(
        safeExec('ls', ['-la', 'path\0injected'], { cwd: process.cwd() })
      ).rejects.toThrow('Argument validation failed');
    });

    it('should reject arguments exceeding max length', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const longArg = 'a'.repeat(1001);

      await expect(
        safeExec('ls', [longArg], { cwd: process.cwd() })
      ).rejects.toThrow('Argument validation failed');
    });

    it('should not forward non-allowlisted env overrides to child processes', async () => {
      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');

      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        env: { GITHUB_TOKEN: 'should-not-pass' },
      });

      setTimeout(() => {
        mockProcess.emit('close', 0);
      }, 10);

      await promise;

      const spawnCall = vi.mocked(spawn).mock.calls[0];
      const spawnOptions = spawnCall?.[2];
      expect(spawnOptions?.env?.GITHUB_TOKEN).toBeUndefined();
    });
  });

  describe('close event when already killed', () => {
    it('should ignore close event when process was already killed', async () => {
      vi.useFakeTimers();

      const { safeExec } =
        await import('../../../../octocode-tools-core/src/utils/exec/safe.js');
      const promise = safeExec('ls', ['-la'], {
        cwd: process.cwd(),
        timeout: 100,
      });

      const resultPromise = promise.catch(e => e);

      await vi.advanceTimersByTimeAsync(150);

      mockProcess.emit('close', 0);

      const error = await resultPromise;
      expect(error).toBeInstanceOf(Error);
      expect(error.message).toContain('Command timeout');

      vi.useRealTimers();
    });
  });
});
