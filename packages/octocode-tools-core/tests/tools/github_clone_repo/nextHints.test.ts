import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

type SpawnResult = {
  stdout: string;
  stderr: string;
  exitCode: number;
  success: boolean;
};

type SpawnMock = (
  command: string,
  args: string[],
  options?: unknown
) => Promise<SpawnResult>;

const mocks = vi.hoisted(() => ({
  octocodeDir: '',
  spawnWithTimeout: vi.fn<SpawnMock>(),
}));

vi.mock('../../../src/shared/index.js', async importOriginal => {
  const actual = await importOriginal<
    typeof import('../../../src/shared/index.js')
  >();
  return {
    ...actual,
    getOctocodeDir: () => mocks.octocodeDir,
  };
});

vi.mock('../../../src/utils/exec/spawn.js', () => ({
  TOOLING_ALLOWED_ENV_VARS: [],
  spawnWithTimeout: (...args: Parameters<SpawnMock>) =>
    mocks.spawnWithTimeout(...args),
}));

const { executeCloneRepo } = await import(
  '../../../src/tools/github_clone_repo/execution.js'
);

describe('ghCloneRepo next-hints', () => {
  beforeEach(() => {
    mocks.octocodeDir = mkdtempSync(join(tmpdir(), 'octocode-clone-next-'));
    mocks.spawnWithTimeout.mockReset();
    mocks.spawnWithTimeout.mockImplementation(async (_command, args) => {
      if (args.includes('clone')) {
        const targetDir = args.at(-1);
        if (targetDir) mkdirSync(targetDir, { recursive: true });
      }
      return { stdout: '', stderr: '', exitCode: 0, success: true };
    });
  });

  afterEach(() => {
    rmSync(mocks.octocodeDir, { recursive: true, force: true });
  });

  it('emits a ready-to-run viewStructure hint and no longer emits the broken localSearch hint (regression)', async () => {
    const result = await executeCloneRepo({
      queries: [{ owner: 'bgauryy', repo: 'octocode', branch: 'main' }],
    } as never);

    const data = (result.structuredContent ?? result) as {
      results: Array<{ data: { next?: Record<string, unknown> } }>;
    };
    const next = data.results[0]?.data.next;
    expect(next?.viewStructure).toBeDefined();
    // Regression: this hint used to be next.localSearch with mode:"discovery"
    // and no keywords, which localSearchCode's core schema always rejects.
    expect(next?.localSearch).toBeUndefined();
    expect(JSON.stringify(next)).not.toContain('"mode":"discovery"');
  });
});
