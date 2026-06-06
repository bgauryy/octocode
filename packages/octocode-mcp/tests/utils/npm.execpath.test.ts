import { describe, it, expect, vi, beforeEach } from 'vitest';
import { dirname, join } from 'path';
import { spawn } from 'child_process';
import { EventEmitter } from 'events';

import {
  executeNpmCommand,
  checkNpmAvailability,
} from '../../src/utils/exec/npm.js';

class MockChildProcess extends EventEmitter {
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  pid = 12345;
  killed = false;
  exitCode: number | null = null;
  signalCode: string | null = null;

  kill(_signal?: string) {
    this.killed = true;
    return true;
  }
}

const expectedNpmPath = join(
  dirname(process.execPath),
  process.platform === 'win32' ? 'npm.cmd' : 'npm'
);

describe('executeNpmCommand - process.execPath spawn', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should spawn npm using process.execPath as the command (not the npm script directly)', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(
      mockProcess as unknown as ReturnType<typeof spawn>
    );

    const resultPromise = executeNpmCommand('view', ['express', '--json']);

    setTimeout(() => {
      mockProcess.stdout.emit('data', Buffer.from('{}'));
      mockProcess.emit('close', 0);
    }, 10);

    await resultPromise;

    expect(vi.mocked(spawn)).toHaveBeenCalledTimes(1);
    const spawnCall = vi.mocked(spawn).mock.calls[0]!;
    const [command, args] = spawnCall;

    expect(command).toBe(process.execPath);

    expect(args![0]).toBe(expectedNpmPath);

    expect(args![1]).toBe('view');
    expect(args![2]).toBe('express');
    expect(args![3]).toBe('--json');
  });

  it('should pass the npm subcommand and all arguments after the npm script path', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(
      mockProcess as unknown as ReturnType<typeof spawn>
    );

    const resultPromise = executeNpmCommand('search', [
      'lodash',
      '--json',
      '--searchlimit=5',
    ]);

    setTimeout(() => {
      mockProcess.stdout.emit('data', Buffer.from('[]'));
      mockProcess.emit('close', 0);
    }, 10);

    await resultPromise;

    const spawnCall = vi.mocked(spawn).mock.calls[0]!;
    const [command, args] = spawnCall;

    expect(command).toBe(process.execPath);
    expect(args).toEqual([
      expectedNpmPath,
      'search',
      'lodash',
      '--json',
      '--searchlimit=5',
    ]);
  });
});

describe('checkNpmAvailability - process.execPath spawn', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should check npm availability using process.execPath as the command', async () => {
    const mockProcess = new MockChildProcess();
    vi.mocked(spawn).mockReturnValue(
      mockProcess as unknown as ReturnType<typeof spawn>
    );

    const resultPromise = checkNpmAvailability(10000);

    setTimeout(() => {
      mockProcess.stdout.emit('data', Buffer.from('10.0.0'));
      mockProcess.emit('close', 0);
    }, 10);

    const result = await resultPromise;

    expect(result).toBe(true);
    expect(vi.mocked(spawn)).toHaveBeenCalledTimes(1);

    const spawnCall = vi.mocked(spawn).mock.calls[0]!;
    const [command, args] = spawnCall;

    expect(command).toBe(process.execPath);
    expect(args![0]).toBe(expectedNpmPath);
    expect(args![1]).toBe('--version');
  });
});
