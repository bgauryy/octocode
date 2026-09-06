import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

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
  githubApiUrl: undefined as string | undefined,
  spawnWithTimeout: vi.fn<SpawnMock>(),
}));

vi.mock('../../../src/serverConfig.js', () => ({
  getServerConfig: () => ({ githubApiUrl: mocks.githubApiUrl }),
}));

vi.mock('../../../src/shared/paths.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../../src/shared/paths.js')>();
  return { ...actual, getOctocodeDir: () => mocks.octocodeDir };
});

vi.mock('../../../src/utils/exec/spawn/env.js', () => ({
  TOOLING_ALLOWED_ENV_VARS: [],
}));
vi.mock('../../../src/utils/exec/spawn/wrappers.js', () => ({
  spawnWithTimeout: (...args: Parameters<SpawnMock>) =>
    mocks.spawnWithTimeout(...args),
}));

const { cloneRepo } =
  await import('../../../src/tools/github_clone_repo/cloneRepo.js');

describe('cloneRepo sparse checkout', () => {
  beforeEach(() => {
    mocks.octocodeDir = mkdtempSync(join(tmpdir(), 'octocode-clone-test-'));
    mocks.githubApiUrl = undefined;
    mocks.spawnWithTimeout.mockReset();
    mocks.spawnWithTimeout.mockImplementation(async (_command, args) => {
      if (args.includes('clone')) {
        const targetDir = args.at(-1);
        if (targetDir) mkdirSync(targetDir, { recursive: true });
      }
      if (args.includes('sparse-checkout')) {
        const targetDir = args[args.indexOf('-C') + 1];
        const sparsePath = args.at(-1);
        if (targetDir && sparsePath) {
          const checkoutPath = join(targetDir, sparsePath);
          mkdirSync(dirname(checkoutPath), { recursive: true });
          writeFileSync(checkoutPath, '', 'utf-8');
        }
      }
      return {
        stdout: args.includes('rev-parse') ? 'a'.repeat(40) : '',
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

  it('routes public and enterprise clones separately and scopes credentials to the repository', async () => {
    const query = { owner: 'acme', repo: 'widget', branch: 'main' };
    const publicClone = await cloneRepo(query, undefined, 'public-token');
    mocks.githubApiUrl = 'https://git.acme.test/proxy/api/v3/';
    const enterpriseClone = await cloneRepo(
      query,
      undefined,
      'enterprise-token'
    );
    const cached = await cloneRepo(query, undefined, 'enterprise-token');
    expect(enterpriseClone.localPath).not.toBe(publicClone.localPath);
    expect(cached.cached).toBe(true);
    expect(cached.localPath).toBe(enterpriseClone.localPath);
    const calls = mocks.spawnWithTimeout.mock.calls.filter(([, args]) =>
      args.includes('clone')
    );
    expect(calls).toHaveLength(2);
    expect(calls[0]?.[1]).toContain('https://github.com/acme/widget.git');
    expect(calls[1]?.[1]).toContain(
      'https://git.acme.test/proxy/acme/widget.git'
    );
    expect(calls[1]?.[1]).toContain(
      'http.https://git.acme.test/proxy/acme/widget.git.extraHeader=Authorization: Bearer enterprise-token'
    );
    expect(calls[1]?.[1]).not.toContain(
      'http.extraHeader=Authorization: Bearer enterprise-token'
    );
    const authArgs = calls[1]![1].slice(0, 2);
    const scoped = spawnSync(
      'git',
      [
        ...authArgs,
        'config',
        '--get-urlmatch',
        'http.extraHeader',
        'https://git.acme.test/proxy/acme/widget.git/info/refs',
      ],
      { encoding: 'utf8' }
    );
    expect(scoped.status).toBe(0);
    expect(scoped.stdout.trim()).toBe('Authorization: Bearer enterprise-token');
    for (const url of [
      'https://other.test/proxy/acme/widget.git',
      'https://git.acme.test/proxy/acme/other.git',
    ]) {
      const unscoped = spawnSync(
        'git',
        [...authArgs, 'config', '--get-urlmatch', 'http.extraHeader', url],
        {
          encoding: 'utf8',
          env: { GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null' },
        }
      );
      expect(unscoped.stdout).toBe('');
    }
  });

  it('authenticates sparse checkout lazy fetches using the same repository scope', async () => {
    await cloneRepo(
      {
        owner: 'acme',
        repo: 'widget',
        branch: 'main',
        sparsePath: 'README.md',
      },
      undefined,
      'private-token'
    );
    const sparseCall = mocks.spawnWithTimeout.mock.calls.find(([, args]) =>
      args.includes('sparse-checkout')
    );
    expect(sparseCall?.[1]).toContain(
      'http.https://github.com/acme/widget.git.extraHeader=Authorization: Bearer private-token'
    );
  });

  it.each([
    'https://proxy.acme.test/custom-api',
    'http://git.acme.test/api/v3',
    'https://user:secret@git.acme.test/api/v3',
  ])(
    'rejects unsupported or unsafe API endpoints before cloning: %s',
    async githubApiUrl => {
      mocks.githubApiUrl = githubApiUrl;
      await expect(
        cloneRepo(
          { owner: 'acme', repo: 'widget', branch: 'main' },
          undefined,
          'secret-token'
        )
      ).rejects.toThrow(/GitHub API endpoint/);
      expect(
        mocks.spawnWithTimeout.mock.calls.some(([, args]) =>
          args.includes('clone')
        )
      ).toBe(false);
    }
  );

  it('redacts authentication from sparse lazy-fetch failures', async () => {
    const materialize = mocks.spawnWithTimeout.getMockImplementation()!;
    mocks.spawnWithTimeout.mockImplementation(async (...args) => {
      if (args[1].includes('sparse-checkout')) {
        return {
          stdout: '',
          stderr: 'Authorization: Bearer private-token rejected',
          exitCode: 1,
          success: false,
        };
      }
      return materialize(...args);
    });
    await expect(
      cloneRepo(
        {
          owner: 'acme',
          repo: 'widget',
          branch: 'main',
          sparsePath: 'README.md',
        },
        undefined,
        'private-token'
      )
    ).rejects.toThrow('Authorization: Bearer [REDACTED] rejected');
  });
});
