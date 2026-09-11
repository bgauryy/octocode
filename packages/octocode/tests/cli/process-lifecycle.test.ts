import { describe, expect, it, vi } from 'vitest';
import {
  terminateForSignal,
  terminationExitCode,
} from '../../src/cli/process-lifecycle.js';

describe('CLI process lifecycle', () => {
  it.each([
    ['SIGINT', 130],
    ['SIGTERM', 143],
  ] as const)('maps %s to conventional exit code %i', (signal, code) => {
    expect(terminationExitCode(signal)).toBe(code);
  });

  it('writes terminal cleanup only to stderr and exits as interrupted', () => {
    const write = vi.fn();
    const exit = vi.fn((code: number) => {
      throw new Error(`exit:${code}`);
    });

    expect(() =>
      terminateForSignal('SIGINT', {
        stderr: { isTTY: true, write },
        exit,
      })
    ).toThrow('exit:130');
    expect(write).toHaveBeenCalledWith('\x1B[?25h');
    expect(write).toHaveBeenCalledWith('\n  Goodbye! 👋\n');
    expect(exit).toHaveBeenCalledWith(130);
  });

  it('does not contaminate redirected structured output', () => {
    const write = vi.fn();
    const exit = vi.fn((code: number) => {
      throw new Error(`exit:${code}`);
    });

    expect(() =>
      terminateForSignal('SIGTERM', {
        stderr: { isTTY: false, write },
        exit,
      })
    ).toThrow('exit:143');
    expect(write).not.toHaveBeenCalled();
  });
});
