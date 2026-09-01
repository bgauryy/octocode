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
  const actual =
    await importOriginal<typeof import('../../../src/shared/index.js')>();
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

const { executeCloneRepo } =
  await import('../../../src/tools/github_clone_repo/execution.js');

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
      results: Array<{
        index: number;
        cache?: 1;
        data: {
          owner: string;
          repo: string;
          totalSize: number;
          location: Record<string, unknown>;
          next?: Record<string, unknown>;
          localPath?: string;
          resolvedBranch?: string;
          cached?: boolean;
        };
      }>;
    };
    const row = data.results[0];
    expect(row?.index).toBe(0);
    expect(Object.keys(row?.data ?? {})).toEqual([
      'owner',
      'repo',
      'totalSize',
      'location',
      'next',
    ]);
    expect(row?.data.location).toMatchObject({
      kind: 'repo',
      source: 'clone',
      complete: true,
      resolvedBranch: 'main',
    });
    expect(row?.data.localPath).toBeUndefined();
    expect(row?.data.resolvedBranch).toBeUndefined();
    expect(row?.data.cached).toBeUndefined();

    const next = row?.data.next;
    expect(next?.viewStructure).toBeDefined();
    // Regression: this hint used to be next.localSearch with mode:"discovery"
    // and no keywords, which local.text's core schema always rejects.
    expect(next?.localSearch).toBeUndefined();
    expect(JSON.stringify(next)).not.toContain('"mode":"discovery"');

    const cachedResult = await executeCloneRepo({
      queries: [{ owner: 'bgauryy', repo: 'octocode', branch: 'main' }],
    } as never);
    const cachedData = (cachedResult.structuredContent ?? cachedResult) as {
      results: Array<{
        cache?: 1;
        data: { location: Record<string, unknown> };
      }>;
    };
    expect(cachedData.results[0]).toMatchObject({
      cache: 1,
      data: { location: { cached: true } },
    });
  });
});
