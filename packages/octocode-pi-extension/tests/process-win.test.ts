import { afterEach, describe, expect, it, vi } from 'vitest';

const spawn = vi.fn();
vi.mock('node:child_process', () => ({ spawn: (...args: unknown[]) => spawn(...args) }));
const { signalTree } = await import('../src/shared/process.js');

const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
afterEach(() => {
  Object.defineProperty(process, 'platform', platform);
  spawn.mockReset();
});

describe('signalTree', () => {
  it('does nothing without a pid', () => {
    const kill = vi.spyOn(process, 'kill');
    signalTree(undefined, 'SIGTERM');
    signalTree(0, 'SIGTERM');
    expect(kill).not.toHaveBeenCalled();
    expect(spawn).not.toHaveBeenCalled();
    kill.mockRestore();
  });

  it('uses taskkill on Windows and never throws when it is missing', () => {
    Object.defineProperty(process, 'platform', { value: 'win32' });
    const on = vi.fn();
    spawn.mockReturnValue({ on });
    signalTree(1234, 'SIGTERM');
    expect(spawn).toHaveBeenCalledWith('taskkill', ['/pid', '1234', '/T', '/F'], { stdio: 'ignore', windowsHide: true });
    expect(on).toHaveBeenCalledWith('error', expect.any(Function));
    spawn.mockImplementation(() => {
      throw new Error('ENOENT');
    });
    expect(() => signalTree(1234, 'SIGKILL')).not.toThrow();
  });
});
