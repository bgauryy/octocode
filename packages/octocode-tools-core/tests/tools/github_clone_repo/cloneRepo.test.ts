import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
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

const { cloneRepo } = await import(
  '../../../src/tools/github_clone_repo/cloneRepo.js'
);

describe('cloneRepo sparse checkout', () => {
  beforeEach(() => {
    mocks.octocodeDir = mkdtempSync(join(tmpdir(), 'octocode-clone-test-'));
    mocks.spawnWithTimeout.mockReset();
    mocks.spawnWithTimeout.mockImplementation(async (_command, args) => {
      if (args.includes('clone')) {
        const targetDir = args.at(-1);
        if (targetDir) mkdirSync(targetDir, { recursive: true });
      }
      if (args.includes('sparse-checkout')) {
        const targetDir = args[1];
        const sparsePath = args.at(-1);
        if (targetDir && sparsePath) {
          const checkoutPath = join(targetDir, sparsePath);
          mkdirSync(dirname(checkoutPath), { recursive: true });
          writeFileSync(checkoutPath, '', 'utf-8');
        }
      }
      return {
        stdout: '',
        stderr: '',
        exitCode: 0,
        success: true,
      };
    });
  });

  afterEach(() => {
    rmSync(mocks.octocodeDir, { recursive: true, force: true });
  });

  it('allows sparse checkout of a single file path', async () => {
    const result = await cloneRepo({
      owner: 'bgauryy',
      repo: 'octocode',
      branch: 'main',
      sparsePath: 'README.md',
    });

    expect(result.localPath).toContain(join('tmp', 'clone'));

    const sparseCall = mocks.spawnWithTimeout.mock.calls.find(([, args]) =>
      args.includes('sparse-checkout')
    );

    expect(sparseCall?.[1]).toEqual(
      expect.arrayContaining(['set', '--skip-checks', '--', 'README.md'])
    );
  });

  it('serializes parallel materializations and promotes one cache entry', async () => {
    const [first, second] = await Promise.all([
      cloneRepo({
        owner: 'bgauryy',
        repo: 'octocode',
        branch: 'main',
      }),
      cloneRepo({
        owner: 'bgauryy',
        repo: 'octocode',
        branch: 'main',
      }),
    ]);

    const cloneCalls = mocks.spawnWithTimeout.mock.calls.filter(([, args]) =>
      args.includes('clone')
    );

    expect(cloneCalls).toHaveLength(1);
    expect([first.cached, second.cached].filter(Boolean)).toHaveLength(1);
    expect(first.localPath).toBe(second.localPath);
    expect(first.localPath).not.toContain('.tmp-');
  });
});
