import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { spawn, ChildProcess } from 'child_process';
import { EventEmitter } from 'events';

import {
  executeNpmCommand,
  checkNpmAvailability,
} from '../../src/utils/exec/npm.js';

// Mock process for testing
class MockChildProcess extends EventEmitter {
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  pid = 12345;
  killed = false;
  exitCode: number | null = null;
  signalCode: string | null = null;

  kill(signal?: string) {
    this.killed = true;
    this.signalCode = signal || 'SIGTERM';
    // Simulate async kill
    setTimeout(() => {
      this.emit('close', null, signal);
    }, 10);
    return true;
  }

  // Simulate successful execution
  simulateSuccess(stdout = '', stderr = '') {
    setTimeout(() => {
      if (stdout) this.stdout.emit('data', stdout);
      if (stderr) this.stderr.emit('data', stderr);
      this.exitCode = 0;
      this.emit('close', 0);
    }, 10);
  }

  // Simulate failure
  simulateFailure(exitCode = 1, stderr = '', stdout = '') {
    setTimeout(() => {
      if (stdout) this.stdout.emit('data', stdout);
      if (stderr) this.stderr.emit('data', stderr);
      this.exitCode = exitCode;
      this.emit('close', exitCode);
    }, 10);
  }

  // Simulate error during spawn
  simulateError(error: Error) {
    setTimeout(() => {
      this.emit('error', error);
    }, 10);
  }

  // Simulate timeout (no close event)
  simulateTimeout() {
    // Just emit data but never close
    setTimeout(() => {
      this.stdout.emit('data', 'some output');
    }, 10);
  }
}

describe('exec utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
  });

  describe('executeNpmCommand', () => {
    let mockProcess: MockChildProcess;

    beforeEach(() => {
      vi.clearAllMocks();
      mockProcess = new MockChildProcess();
      vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);
    });

    it('should execute allowed npm command and return stdout', async () => {
      const promise = executeNpmCommand('search', ['axios', '--json']);
      mockProcess.simulateSuccess('[{"name": "axios"}]', '');

      const result = await promise;

      expect(result.stdout).toBe('[{"name": "axios"}]');
      expect(result.stderr).toBe('');
      expect(result.error).toBeUndefined();
      expect(result.exitCode).toBe(0);
    });

    it('should reject non-allowed npm commands', async () => {
      const result = await executeNpmCommand('install' as 'search', [
        'some-package',
      ]);

      expect(result.error).toBeDefined();
      expect(result.error?.message).toContain('not allowed');
    });

    it('should validate arguments for null bytes', async () => {
      const result = await executeNpmCommand('search', ['package\0name']);

      expect(result.error).toBeDefined();
      expect(result.error?.message).toContain('Null bytes');
    });

    it('should validate arguments for excessive length', async () => {
      const longArg = 'a'.repeat(1001);
      const result = await executeNpmCommand('search', [longArg]);

      expect(result.error).toBeDefined();
      expect(result.error?.message).toContain('too long');
    });

    it('should handle non-zero exit code', async () => {
      const promise = executeNpmCommand('search', ['nonexistent']);
      mockProcess.simulateFailure(1, 'ERR: not found');

      const result = await promise;

      expect(result.exitCode).toBe(1);
      expect(result.stderr).toBe('ERR: not found');
    });

    it('should handle spawn error', async () => {
      const promise = executeNpmCommand('search', ['axios']);
      mockProcess.simulateError(new Error('ENOENT: npm not found'));

      const result = await promise;

      expect(result.error).toBeDefined();
      expect(result.error?.message).toBe('ENOENT: npm not found');
    });

    it('should handle timeout', async () => {
      vi.useFakeTimers();

      const promise = executeNpmCommand('search', ['axios'], { timeout: 1000 });
      mockProcess.simulateTimeout();

      await vi.advanceTimersByTimeAsync(1000);

      const result = await promise;

      expect(result.error).toBeDefined();
      expect(result.error?.message).toMatch(/^Command timeout/);
      expect(mockProcess.killed).toBe(true);

      vi.useRealTimers();
    });

    it('should remove dangerous environment variables', async () => {
      const promise = executeNpmCommand('search', ['axios']);
      mockProcess.simulateSuccess('[]');

      await promise;

      const spawnCall = vi.mocked(spawn).mock.calls[0];
      const spawnOptions = spawnCall?.[2];

      expect(spawnOptions?.env?.NODE_OPTIONS).toBeUndefined();
      expect(spawnOptions?.env?.NPM_CONFIG_SCRIPT_SHELL).toBeUndefined();
    });

    it('should allow all valid npm commands', async () => {
      const validCommands = ['view', 'search', 'ping', 'config', 'whoami'];

      for (const cmd of validCommands) {
        const promise = executeNpmCommand(cmd as 'search', ['test']);
        mockProcess.simulateSuccess('result');
        await promise;

        // Verify spawn was called (no early error return)
        expect(vi.mocked(spawn)).toHaveBeenCalled();
        vi.clearAllMocks();
        mockProcess = new MockChildProcess();
        vi.mocked(spawn).mockReturnValue(
          mockProcess as unknown as ChildProcess
        );
      }
    });

    it('should pass arguments without shell escaping', async () => {
      const promise = executeNpmCommand('search', ['package$name']);
      mockProcess.simulateSuccess('[]');

      await promise;

      const spawnCall = vi.mocked(spawn).mock.calls[0];
      const args = spawnCall?.[1];

      // Verify dollar sign is NOT escaped
      expect(args).toContain('package$name');
    });

    it('should accumulate stdout from multiple data events', async () => {
      const promise = executeNpmCommand('search', ['axios']);

      setTimeout(() => {
        mockProcess.stdout.emit('data', '[{"name":');
        mockProcess.stdout.emit('data', '"axios"}]');
        mockProcess.emit('close', 0);
      }, 10);

      const result = await promise;

      expect(result.stdout).toBe('[{"name":"axios"}]');
    });

    it('should accumulate stderr from multiple data events', async () => {
      const promise = executeNpmCommand('search', ['test']);

      setTimeout(() => {
        mockProcess.stderr.emit('data', 'warn: ');
        mockProcess.stderr.emit('data', 'some warning');
        mockProcess.emit('close', 0);
      }, 10);

      const result = await promise;

      expect(result.stderr).toBe('warn: some warning');
    });
  });

  describe('checkNpmAvailability', () => {
    let mockProcess: MockChildProcess;

    beforeEach(() => {
      vi.clearAllMocks();
      mockProcess = new MockChildProcess();
      vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);
    });

    it('should return true when npm --version succeeds', async () => {
      const promise = checkNpmAvailability();
      mockProcess.simulateSuccess('10.0.0');

      const result = await promise;

      expect(result).toBe(true);
      // npm is now spawned via process.execPath to bypass shebang PATH issues
      expect(vi.mocked(spawn)).toHaveBeenCalledWith(
        process.execPath,
        [expect.stringMatching(/npm(\.cmd)?$/), '--version'],
        expect.objectContaining({
          timeout: 10000,
        })
      );
    });

    it('should return false when npm --version fails with non-zero exit code', async () => {
      const promise = checkNpmAvailability();
      mockProcess.simulateFailure(1, 'ERR! command not found');

      const result = await promise;

      expect(result).toBe(false);
    });

    it('should return false when npm --version encounters an error', async () => {
      const promise = checkNpmAvailability();
      mockProcess.simulateError(new Error('ENOENT: npm not found'));

      const result = await promise;

      expect(result).toBe(false);
    });

    it('should return false when npm --version times out', async () => {
      vi.useFakeTimers();

      const promise = checkNpmAvailability(5000);
      mockProcess.simulateTimeout();

      await vi.advanceTimersByTimeAsync(5000);

      const result = await promise;

      expect(result).toBe(false);
      expect(mockProcess.killed).toBe(true);

      vi.useRealTimers();
    });

    it('should use custom timeout when provided', async () => {
      const promise = checkNpmAvailability(15000);
      mockProcess.simulateSuccess('10.0.0');

      await promise;

      // npm is now spawned via process.execPath to bypass shebang PATH issues
      expect(vi.mocked(spawn)).toHaveBeenCalledWith(
        process.execPath,
        [expect.stringMatching(/npm(\.cmd)?$/), '--version'],
        expect.objectContaining({
          timeout: 15000,
        })
      );
    });

    it('should use default timeout of 10 seconds', async () => {
      const promise = checkNpmAvailability();
      mockProcess.simulateSuccess('10.0.0');

      await promise;

      // npm is now spawned via process.execPath to bypass shebang PATH issues
      expect(vi.mocked(spawn)).toHaveBeenCalledWith(
        process.execPath,
        [expect.stringMatching(/npm(\.cmd)?$/), '--version'],
        expect.objectContaining({
          timeout: 10000,
        })
      );
    });
  });
});
