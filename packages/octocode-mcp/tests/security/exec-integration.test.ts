import { describe, it, expect, vi, beforeAll } from 'vitest';
import path from 'path';
import { spawn } from 'child_process';
import type { ExecResult } from '../../src/utils/core/types.js';

let safeExec: (
  command: string,
  args?: string[],
  options?: { cwd?: string; timeout?: number }
) => Promise<ExecResult>;

beforeAll(async () => {
  const childProcess =
    await vi.importActual<typeof import('child_process')>('child_process');
  vi.mocked(spawn).mockImplementation(childProcess.spawn);

  const safeModule = await import('../../src/utils/exec/safe.js');
  safeExec = safeModule.safeExec;
});

describe('safeExec execution context security', () => {
  const workspaceRoot = process.cwd();
  const parentDir = path.dirname(workspaceRoot);

  it('should allow execution within workspace', async () => {
    const result = await safeExec('ls', ['-la'], {
      cwd: workspaceRoot,
    });
    expect(result.success).toBe(true);
  });

  it('should allow execution in subdirectory of workspace', async () => {
    const srcPath = path.join(workspaceRoot, 'src');
    const result = await safeExec('ls', ['-la'], {
      cwd: srcPath,
    });
    expect(result.success).toBe(true);
  });

  it('allows execution in parent directory (cwd sandbox removed)', async () => {
    const result = await safeExec('ls', ['-la'], {
      cwd: parentDir,
    });
    expect(result.success).toBe(true);
  });

  it('allows execution in a system directory (cwd sandbox removed)', async () => {
    const result = await safeExec('ls', ['-la'], {
      cwd: '/etc',
    });
    expect(result.success).toBe(true);
  });

  it('should allow execution with undefined cwd (defaults to safe)', async () => {
    const result = await safeExec('ls', ['-la']);
    expect(result.success).toBe(true);
  });
});
