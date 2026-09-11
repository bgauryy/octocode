import { afterEach, describe, expect, it, vi } from 'vitest';
import { readGitStatus } from '../src/git.js';

const { spawnSync } = vi.hoisted(() => ({ spawnSync: vi.fn() }));
vi.mock('node:child_process', async importOriginal => ({
  ...await importOriginal<Record<string, unknown>>(), spawnSync,
}));
afterEach(() => { spawnSync.mockReset(); });

describe('Git status failure boundaries', () => {
  it.each([' M truncated.ts', 'R  renamed.ts\0'])('rejects incomplete NUL records %j', output => {
    spawnSync.mockReturnValue({ status: 0, stdout: output, stderr: '' });
    expect(() => readGitStatus('/fixture')).toThrow(/incomplete/);
  });
});
