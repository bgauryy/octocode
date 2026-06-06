import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EventEmitter } from 'events';
import { ChildProcess, spawn } from 'child_process';

import {
  buildChildProcessEnv,
  spawnCheckSuccess,
  spawnCollectStdout,
  spawnWithTimeout,
  validateArgs,
} from '../../src/utils/exec/spawn.js';

class MockChildProcess extends EventEmitter {
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  killed = false;

  kill() {
    this.killed = true;
    setTimeout(() => this.emit('close', null), 0);
    return true;
  }
}

describe('spawn env and memory hardening', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
  });

  it('buildChildProcessEnv should only inherit allowlisted vars from process.env', () => {
    process.env.PATH = '/usr/bin';
    process.env.HOME = '/Users/test';
    process.env.SECRET_TOKEN = 'secret';

    const env = buildChildProcessEnv({}, ['PATH', 'HOME'] as const);

    expect(env.PATH).toBe('/usr/bin');
    expect(env.HOME).toBe('/Users/test');
    expect(env.SECRET_TOKEN).toBeUndefined();
  });

  it('buildChildProcessEnv should only apply overrides for keys in the allowlist', () => {
    process.env.PATH = '/usr/bin';

    const env = buildChildProcessEnv(
      { GIT_TERMINAL_PROMPT: '0', CUSTOM_FLAG: '1', PATH: '/custom/path' },
      ['PATH'] as const
    );

    expect(env.PATH).toBe('/custom/path');
    expect(env.GIT_TERMINAL_PROMPT).toBeUndefined();
    expect(env.CUSTOM_FLAG).toBeUndefined();
  });

  it('buildChildProcessEnv should not inherit sensitive vars from process.env', () => {
    process.env.PATH = '/usr/bin';
    process.env.GITHUB_TOKEN = 'ghp_secret';
    process.env.AWS_SECRET_ACCESS_KEY = 'aws_secret';

    const env = buildChildProcessEnv({}, ['PATH'] as const);

    expect(env.PATH).toBe('/usr/bin');
    expect(env.GITHUB_TOKEN).toBeUndefined();
    expect(env.AWS_SECRET_ACCESS_KEY).toBeUndefined();
  });

  it('spawnCheckSuccess should use minimal default env (no proxy by default)', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);
    process.env.HTTP_PROXY = 'http://proxy.internal:8080';

    const promise = spawnCheckSuccess('npm', ['--version'], 1000);
    setTimeout(() => mockProcess.emit('close', 0), 0);
    const result = await promise;

    const spawnOptions = vi.mocked(spawn).mock.calls[0]?.[2];
    expect(result).toBe(true);
    expect(spawnOptions?.env?.HTTP_PROXY).toBeUndefined();
  });

  it('spawnCollectStdout should include tooling profile envs by default', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);
    process.env.HOME = '/Users/tooling-home';

    const promise = spawnCollectStdout('gh', ['auth', 'token'], 1000);
    setTimeout(() => {
      mockProcess.stdout.emit('data', 'token-123');
      mockProcess.emit('close', 0);
    }, 0);
    const result = await promise;

    const spawnOptions = vi.mocked(spawn).mock.calls[0]?.[2];
    expect(result).toBe('token-123');
    expect(spawnOptions?.env?.HOME).toBe('/Users/tooling-home');
  });

  it('spawnWithTimeout should enforce default output limit to bound memory usage', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(mockProcess as unknown as ChildProcess);

    const promise = spawnWithTimeout('dummy', []);

    setTimeout(() => {
      mockProcess.stdout.emit('data', Buffer.alloc(11 * 1024 * 1024, 'a'));
    }, 0);

    const result = await promise;
    expect(result.success).toBe(false);
    expect(result.outputLimitExceeded).toBe(true);
  });
});
describe('spawn utilities - validateArgs', () => {
  describe('validateArgs', () => {
    it('should accept valid arguments', () => {
      const result = validateArgs(['arg1', 'arg2', 'arg3']);

      expect(result.valid).toBe(true);
      expect(result.error).toBeUndefined();
    });

    it('should reject arguments with null bytes', () => {
      const result = validateArgs(['valid', 'has\0null', 'also valid']);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('Null bytes');
    });

    it('should reject arguments with null byte at start', () => {
      const result = validateArgs(['\0test']);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('Null bytes');
    });

    it('should reject arguments with null byte at end', () => {
      const result = validateArgs(['test\0']);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('Null bytes');
    });

    it('should reject arguments with multiple null bytes', () => {
      const result = validateArgs(['te\0st\0']);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('Null bytes');
    });

    it('should reject arguments that are too long', () => {
      const longArg = 'a'.repeat(1001);
      const result = validateArgs([longArg]);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('too long');
    });

    it('should accept arguments at exact max length', () => {
      const maxArg = 'a'.repeat(1000);
      const result = validateArgs([maxArg]);

      expect(result.valid).toBe(true);
    });

    it('should use custom max length', () => {
      const result = validateArgs(['short', 'medium'], 5);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('too long');
    });

    it('should accept arguments within custom max length', () => {
      const result = validateArgs(['ab'], 2);

      expect(result.valid).toBe(true);
    });

    it('should accept empty arguments array', () => {
      const result = validateArgs([]);

      expect(result.valid).toBe(true);
    });

    it('should accept empty strings as arguments', () => {
      const result = validateArgs(['', 'valid']);

      expect(result.valid).toBe(true);
    });

    it('should accept single empty string argument', () => {
      const result = validateArgs(['']);

      expect(result.valid).toBe(true);
    });

    it('should validate all arguments in array', () => {
      expect(validateArgs(['has\0null', 'valid', 'valid']).valid).toBe(false);

      expect(validateArgs(['valid', 'valid', 'has\0null']).valid).toBe(false);

      expect(validateArgs(['valid', 'has\0null', 'valid']).valid).toBe(false);
    });

    it('should validate length of all arguments in array', () => {
      const longArg = 'a'.repeat(1001);

      expect(validateArgs([longArg, 'short', 'short']).valid).toBe(false);

      expect(validateArgs(['short', 'short', longArg]).valid).toBe(false);

      expect(validateArgs(['short', longArg, 'short']).valid).toBe(false);
    });

    it('should handle unicode characters correctly', () => {
      const result = validateArgs(['こんにちは', '你好', '🎉']);

      expect(result.valid).toBe(true);
    });

    it('should count length by JavaScript string length (code units)', () => {
      const unicodeArg = '🎉'.repeat(500);
      const result = validateArgs([unicodeArg]);

      expect(result.valid).toBe(true);

      const tooLong = '🎉'.repeat(501);
      expect(validateArgs([tooLong]).valid).toBe(false);
    });

    it('should handle special characters correctly', () => {
      const result = validateArgs([
        'path/to/file',
        '--flag=value',
        '-x',
        'arg with spaces',
        '"quoted"',
        "'single quoted'",
        'back\\slash',
        'tab\there',
        'newline\nhere',
      ]);

      expect(result.valid).toBe(true);
    });

    it('should return specific error for null bytes', () => {
      const result = validateArgs(['has\0null']);

      expect(result.valid).toBe(false);
      expect(result.error).toBe('Null bytes not allowed in arguments');
    });

    it('should return specific error for too long arguments', () => {
      const longArg = 'a'.repeat(1001);
      const result = validateArgs([longArg]);

      expect(result.valid).toBe(false);
      expect(result.error).toBe('Argument too long');
    });

    it('should check null bytes before length', () => {
      const longWithNull = 'a'.repeat(1001) + '\0';
      const result = validateArgs([longWithNull]);

      expect(result.valid).toBe(false);
      expect(result.error).toContain('Null bytes');
    });

    it('should handle very large arrays', () => {
      const manyArgs = Array.from({ length: 1000 }, (_, i) => `arg${i}`);
      const result = validateArgs(manyArgs);

      expect(result.valid).toBe(true);
    });

    it('should handle custom max length of 0', () => {
      expect(validateArgs([''], 0).valid).toBe(true);
      expect(validateArgs(['a'], 0).valid).toBe(false);
    });
  });
});
