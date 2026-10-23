import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  STARTUP_FAILURE_REPLY_MS,
  failStartup,
} from '../../src/startup-failure.js';

describe('failStartup', () => {
  let written: string[];
  const stderr = vi.fn();
  const exit = vi.fn();

  beforeEach(() => {
    vi.useFakeTimers();
    written = [];
    vi.spyOn(process.stderr, 'write').mockImplementation(chunk => {
      stderr(chunk);
      return true;
    });
    vi.spyOn(process.stdout, 'write').mockImplementation(
      (chunk: string | Uint8Array, callback?: unknown) => {
        written.push(String(chunk));
        if (typeof callback === 'function') callback();
        return true;
      }
    );
    vi.spyOn(process, 'exit').mockImplementation(code => {
      exit(code);
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- process.exit returns never; the stub must return so the timers and events under test keep running.
      return undefined as never;
    });
  });
  afterEach(() => {
    process.stdin.removeAllListeners('data');
    process.stdin.removeAllListeners('end');
    process.stdin.removeAllListeners('error');
    vi.useRealTimers();
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  it('answers the first request with the reason, then exits 1', () => {
    failStartup('Server initialization failed: boom');
    process.stdin.emit('data', 'not json\n{"jsonrpc":"2.0","method":"x"}\n');
    expect(written).toEqual([]);

    process.stdin.emit(
      'data',
      '{"jsonrpc":"2.0","id":7,"method":"initialize"}\n{"jsonrpc":"2.0","id":8,"method":"ping"}\n'
    );

    expect(stderr).toHaveBeenCalledWith('Server initialization failed: boom\n');
    expect(written.map(line => JSON.parse(line))).toEqual([
      {
        jsonrpc: '2.0',
        id: 7,
        error: { code: -32603, message: 'Server initialization failed: boom' },
      },
    ]);
    expect(exit).toHaveBeenCalledWith(1);
  });

  it('exits after the reply window without a request', () => {
    failStartup('Server initialization failed: boom');
    vi.advanceTimersByTime(STARTUP_FAILURE_REPLY_MS);
    expect(exit).toHaveBeenCalledWith(1);
    expect(written).toEqual([]);
  });

  it('exits when stdin closes', () => {
    failStartup('Server initialization failed: boom');
    process.stdin.emit('end');
    expect(exit).toHaveBeenCalledWith(1);
  });
});
