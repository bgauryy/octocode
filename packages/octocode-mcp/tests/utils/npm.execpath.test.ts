import { describe, it, expect, vi, beforeEach } from 'vitest';
import { dirname, join } from 'path';
import { spawn } from 'child_process';
import { EventEmitter } from 'events';

import {
  executeNpmCommand,
  checkNpmAvailability,
} from '../../../octocode-tools-core/src/utils/exec/npm.js';

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

function expectNpmInvocation(
  command: unknown,
  args: unknown[] | undefined,
  npmArgs: string[]
) {
  if (command === process.execPath) {
    expect(args?.[0]).toBe(expectedNpmPath);
    expect(args?.slice(1)).toEqual(npmArgs);
    return;
  }

  expect(String(command)).toMatch(/npm(\.cmd)?$/);
  expect(args).toEqual(npmArgs);
}

describe('executeNpmCommand - npm invocation spawn', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should spawn a resolved npm invocation', async () => {
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

    expectNpmInvocation(command, args, ['view', 'express', '--json']);
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

    expectNpmInvocation(command, args, [
      'search',
      'lodash',
      '--json',
      '--searchlimit=5',
    ]);
  });
});

describe('checkNpmAvailability - npm invocation spawn', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('should check npm availability using a resolved npm invocation', async () => {
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

    expectNpmInvocation(command, args, ['--version']);
  });
});
