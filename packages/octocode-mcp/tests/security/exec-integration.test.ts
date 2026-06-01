/**
 * Integration test demonstrating execution context security
 */

import { describe, it, expect, vi, beforeAll } from 'vitest';
import path from 'path';
import { spawn } from 'child_process';
import type { ExecResult } from '../../src/utils/core/types.js';

// We need to manually reset the spawn mock to use real implementation
// because vi.unmock doesn't work after module is already loaded
let safeExec: (
  command: string,
  args?: string[],
  options?: { cwd?: string; timeout?: number }
) => Promise<ExecResult>;

beforeAll(async () => {
  // Reset the spawn mock to use real child_process.spawn
  const childProcess =
    await vi.importActual<typeof import('child_process')>('child_process');
  vi.mocked(spawn).mockImplementation(childProcess.spawn);

  // Now import safeExec which will use the real spawn
  const safeModule = await import('../../src/utils/exec/safe.js');
  safeExec = safeModule.safeExec;
});

describe('safeExec execution context security', () => {
  const workspaceRoot = process.cwd();
  const parentDir = path.dirname(workspaceRoot);

  it('should allow execution within workspace', async () => {
    // This should work - executing in workspace
    const result = await safeExec('ls', ['-la'], {
      cwd: workspaceRoot,
    });
    expect(result.success).toBe(true);
  });

  it('should allow execution in subdirectory of workspace', async () => {
    // This should work - executing in workspace subdirectory
    const srcPath = path.join(workspaceRoot, 'src');
    const result = await safeExec('ls', ['-la'], {
      cwd: srcPath,
    });
    expect(result.success).toBe(true);
  });

  // The WORKSPACE_ROOT command-cwd sandbox was removed: command execution is
  // no longer confined to the workspace. Only command + argument validation
  // remain as guards. These cases used to be blocked and now run normally.
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
    // This should work - undefined cwd is safe (uses process.cwd())
    const result = await safeExec('ls', ['-la']);
    expect(result.success).toBe(true);
  });
});
