/**
 * Branch coverage tests for lsp_find_references/lspReferencesProcess.ts
 *
 * Covers all branches of the exported `spawnCollectOutput`:
 * - validation failure (with and without error message)
 * - stdout data exceeds maxBuffer → kill + reject
 * - close code 0 → resolve
 * - close code 1 → resolve
 * - close code other → reject with code
 * - error event → reject
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockKill = vi.fn();
const mockStdoutOn = vi.fn();
const mockSpawnOn = vi.fn();

const mockSpawnReturn = {
  stdout: { on: mockStdoutOn },
  on: mockSpawnOn,
  kill: mockKill,
};

vi.mock('child_process', () => ({
  spawn: vi.fn(() => mockSpawnReturn),
}));

vi.mock('octocode-security-utils/commandValidator', () => ({
  validateCommand: vi.fn(),
}));

import { validateCommand } from 'octocode-security-utils/commandValidator';
import { spawnCollectOutput } from '../../src/tools/lsp_find_references/lspReferencesProcess.js';

describe('spawnCollectOutput (lspReferencesProcess)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('throws when validation fails with an error message', async () => {
    vi.mocked(validateCommand).mockReturnValue({
      isValid: false,
      error: 'command not on allowlist',
    });

    await expect(spawnCollectOutput('bad-cmd', ['arg'])).rejects.toThrow(
      'Command validation failed: command not on allowlist'
    );
  });

  it('throws with fallback message when validation fails without error field', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: false });

    await expect(spawnCollectOutput('bad-cmd', [])).rejects.toThrow(
      'Command validation failed: Command not allowed'
    );
  });

  it('resolves with accumulated stdout when close code is 0', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(
      (event: string, cb: (data: Buffer) => void) => {
        if (event === 'data') {
          cb(Buffer.from('hello '));
          cb(Buffer.from('world'));
        }
      }
    );
    mockSpawnOn.mockImplementation(
      (event: string, cb: (code: number) => void) => {
        if (event === 'close') setTimeout(() => cb(0), 0);
      }
    );

    const result = await spawnCollectOutput('echo', ['hello']);
    expect(result.stdout).toBe('hello world');
  });

  it('resolves with stdout when close code is 1 (ripgrep no-match exit)', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(() => {});
    mockSpawnOn.mockImplementation(
      (event: string, cb: (code: number) => void) => {
        if (event === 'close') setTimeout(() => cb(1), 0);
      }
    );

    const result = await spawnCollectOutput('rg', ['pattern', '/path']);
    expect(result.stdout).toBe('');
  });

  it('rejects when close code is non-zero and non-one', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(() => {});
    mockSpawnOn.mockImplementation(
      (event: string, cb: (code: number) => void) => {
        if (event === 'close') setTimeout(() => cb(2), 0);
      }
    );

    await expect(
      spawnCollectOutput('rg', ['pattern', '/path'])
    ).rejects.toThrow('Process exited with code 2');
  });

  it('rejects and kills child when output exceeds maxBuffer', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(
      (event: string, cb: (data: Buffer) => void) => {
        if (event === 'data') {
          cb(Buffer.alloc(20, 'x'));
        }
      }
    );
    mockSpawnOn.mockImplementation(() => {});

    await expect(
      spawnCollectOutput('echo', [], { maxBuffer: 10 })
    ).rejects.toThrow('Output size limit exceeded');
    expect(mockKill).toHaveBeenCalledWith('SIGKILL');
  });

  it('rejects when child emits an error event', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(() => {});
    mockSpawnOn.mockImplementation(
      (event: string, cb: (err: Error) => void) => {
        if (event === 'error') setTimeout(() => cb(new Error('spawn ENOENT')), 0);
      }
    );

    await expect(spawnCollectOutput('missing-cmd', [])).rejects.toThrow(
      'spawn ENOENT'
    );
  });

  it('respects custom timeout and maxBuffer options', async () => {
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });

    mockStdoutOn.mockImplementation(() => {});
    mockSpawnOn.mockImplementation(
      (event: string, cb: (code: number) => void) => {
        if (event === 'close') setTimeout(() => cb(0), 0);
      }
    );

    const result = await spawnCollectOutput('echo', [], {
      maxBuffer: 512,
      timeout: 5000,
    });
    expect(result.stdout).toBe('');
  });
});
