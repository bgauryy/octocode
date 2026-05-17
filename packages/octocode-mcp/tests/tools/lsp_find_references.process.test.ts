/**
 * Tests for lspReferencesProcess.ts (spawnCollectOutput)
 *
 * Covers:
 * - Command validation failure
 * - Successful stdout collection (exit code 0)
 * - Ripgrep no-match exit code 1 → resolves (not rejects)
 * - Output buffer limit exceeded → rejects
 * - Non-zero exit code (≥2) → rejects
 * - spawn error event → rejects
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';

// Track spawn call args so we can simulate different scenarios
const mockKill = vi.fn();
const mockStdoutOn = vi.fn();
const mockChildOn = vi.fn();

const mockChild = {
  stdout: { on: mockStdoutOn },
  on: mockChildOn,
  kill: mockKill,
};

const mockSpawnFn = vi.fn(() => mockChild);

vi.mock('child_process', () => ({
  spawn: mockSpawnFn,
}));

vi.mock('octocode-security-utils/commandValidator', () => ({
  validateCommand: vi.fn().mockReturnValue({ isValid: true }),
}));

import { validateCommand } from 'octocode-security-utils/commandValidator';
import { spawnCollectOutput } from '../../src/tools/lsp_find_references/lspReferencesProcess.js';

describe('spawnCollectOutput', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(validateCommand).mockReturnValue({ isValid: true });
  });

  const simulateSpawn = (
    dataChunks: Buffer[],
    closeCode: number | null,
    spawnError?: Error
  ) => {
    mockStdoutOn.mockImplementation(
      (event: string, cb: (data: Buffer) => void) => {
        if (event === 'data') {
          for (const chunk of dataChunks) {
            cb(chunk);
          }
        }
      }
    );
    mockChildOn.mockImplementation(
      (event: string, cb: (arg: number | null | Error) => void) => {
        if (event === 'close' && spawnError === undefined) {
          setImmediate(() => cb(closeCode));
        }
        if (event === 'error' && spawnError) {
          setImmediate(() => cb(spawnError));
        }
      }
    );
  };

  it('resolves with stdout on exit code 0', async () => {
    simulateSpawn([Buffer.from('hello\n')], 0);
    const result = await spawnCollectOutput('rg', ['--version']);
    expect(result.stdout).toBe('hello\n');
  });

  it('resolves with stdout on exit code 1 (no matches)', async () => {
    simulateSpawn([Buffer.from('')], 1);
    const result = await spawnCollectOutput('rg', ['pattern', '/path']);
    expect(result.stdout).toBe('');
  });

  it('resolves with concatenated chunks', async () => {
    simulateSpawn(
      [Buffer.from('line1\n'), Buffer.from('line2\n'), Buffer.from('line3\n')],
      0
    );
    const result = await spawnCollectOutput('rg', ['foo']);
    expect(result.stdout).toBe('line1\nline2\nline3\n');
  });

  it('rejects when command validation fails', async () => {
    vi.mocked(validateCommand).mockReturnValue({
      isValid: false,
      error: 'Command not allowed',
    });

    await expect(
      spawnCollectOutput('evil-cmd', ['--bad'])
    ).rejects.toThrow('Command validation failed: Command not allowed');
  });

  it('rejects with generic message when validation error is missing', async () => {
    vi.mocked(validateCommand).mockReturnValue({
      isValid: false,
      error: undefined,
    });

    await expect(
      spawnCollectOutput('evil-cmd', [])
    ).rejects.toThrow('Command validation failed: Command not allowed');
  });

  it('rejects when exit code is ≥ 2', async () => {
    simulateSpawn([], 2);
    await expect(
      spawnCollectOutput('rg', ['pattern'])
    ).rejects.toThrow('Process exited with code 2');
  });

  it('rejects on spawn error event', async () => {
    const spawnErr = new Error('ENOENT: no such file');
    simulateSpawn([], null, spawnErr);
    await expect(
      spawnCollectOutput('rg', ['pattern'])
    ).rejects.toThrow('ENOENT: no such file');
  });

  it('rejects and kills process when output exceeds maxBuffer', async () => {
    // Create a single chunk larger than the custom maxBuffer
    const hugeChunk = Buffer.alloc(200);
    mockStdoutOn.mockImplementation(
      (event: string, cb: (data: Buffer) => void) => {
        if (event === 'data') {
          cb(hugeChunk);
        }
      }
    );
    // close won't be called because kill() is called first - but we need the
    // Promise to reject, so simulate it via the error path after kill
    mockChildOn.mockImplementation(() => {});

    await expect(
      spawnCollectOutput('rg', ['pattern'], { maxBuffer: 100 })
    ).rejects.toThrow('Output size limit exceeded');
    expect(mockKill).toHaveBeenCalledWith('SIGKILL');
  });

  it('uses default maxBuffer and timeout options when not provided', async () => {
    simulateSpawn([Buffer.from('output')], 0);
    const result = await spawnCollectOutput('rg', ['x']);
    expect(result.stdout).toBe('output');
    // spawn was called (options just use defaults)
    expect(mockSpawnFn).toHaveBeenCalled();
  });
});
