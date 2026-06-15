import { describe, it, expect, vi } from 'vitest';
import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import { dirname, join } from 'path';

class MockChildProcess extends EventEmitter {
  stdout = new EventEmitter();
  stderr = new EventEmitter();
  killed = false;

  kill(_signal?: string) {
    this.killed = true;
    return true;
  }
}

describe('executeNpmCommand npm invocation fallback', () => {
  it('uses npm found on PATH when process.execPath has no sibling npm', async () => {
    vi.resetModules();

    const siblingNpm = join(
      dirname(process.execPath),
      process.platform === 'win32' ? 'npm.cmd' : 'npm'
    );
    const pathNpm =
      process.platform === 'win32' ? 'C:\\tools\\npm.cmd' : '/custom/bin/npm';
    const originalPath = process.env.PATH;

    try {
      process.env.PATH =
        process.platform === 'win32' ? 'C:\\tools' : '/custom/bin';

      vi.doMock('fs', () => ({
        existsSync: vi.fn((candidate: string) => {
          if (candidate === siblingNpm) return false;
          return candidate === pathNpm;
        }),
      }));

      const { executeNpmCommand } =
        await import('../../../octocode-tools-core/src/utils/exec/npm.js');

      const mockProcess = new MockChildProcess();
      vi.mocked(spawn).mockReturnValue(
        mockProcess as unknown as ReturnType<typeof spawn>
      );

      const resultPromise = executeNpmCommand('view', ['zod', '--json']);
      setTimeout(() => {
        mockProcess.stdout.emit('data', Buffer.from('{}'));
        mockProcess.emit('close', 0);
      }, 10);

      await resultPromise;

      const [command, args] = vi.mocked(spawn).mock.calls[0]!;
      expect(command).toBe(pathNpm);
      expect(args).toEqual(['view', 'zod', '--json']);
    } finally {
      if (originalPath === undefined) delete process.env.PATH;
      else process.env.PATH = originalPath;
      vi.doUnmock('fs');
    }
  });
});
