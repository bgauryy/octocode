import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const state = vi.hoisted(() => ({
  home: '',
  remote: '',
  failPromotion: false,
  calls: [] as string[][],
}));
vi.mock('../../../src/shared/paths.js', async original => ({
  ...(await original<typeof import('../../../src/shared/paths.js')>()),
  getOctocodeDir: () => state.home,
}));
vi.mock('../../../src/serverConfig.js', () => ({
  getServerConfig: () => ({ githubApiUrl: 'https://api.github.com' }),
}));
vi.mock('fs', async original => {
  const fs = await original<typeof import('fs')>();
  return {
    ...fs,
    renameSync: (from: string, to: string) => {
      if (
        state.failPromotion &&
        from.includes('/clone-tmp/') &&
        !from.endsWith('.previous')
      )
        throw new Error('injected promotion failure');
      return fs.renameSync(from, to);
    },
  };
});
vi.mock('../../../src/utils/exec/spawn/wrappers.js', () => ({
  spawnWithTimeout: async (_command: string, args: string[]) => {
    state.calls.push([...args]);
    // Exercise the installed Git binary against an isolated local remote, not
    // an imitation of fetch/checkout behavior or a network-dependent fixture.
    const result = spawnSync(
      'git',
      args.map(arg =>
        arg === 'https://github.com/acme/fixture.git'
          ? pathToFileURL(state.remote).href
          : arg
      ),
      {
        encoding: 'utf8',
        env: {
          ...process.env,
          GIT_CONFIG_NOSYSTEM: '1',
          GIT_CONFIG_GLOBAL: '/dev/null',
          GIT_TERMINAL_PROMPT: '0',
        },
      }
    );
    return {
      success: result.status === 0,
      exitCode: result.status ?? 1,
      stdout: result.stdout,
      stderr: result.stderr,
    };
  },
}));
const { cloneRepo } =
  await import('../../../src/tools/github_clone_repo/cloneRepo.js');
const { readCacheMeta } =
  await import('../../../src/tools/github_clone_repo/cache.js');
let root: string;
let sha: string;
function git(...args: string[]) {
  const result = spawnSync('git', ['-C', state.remote, ...args], {
    encoding: 'utf8',
    env: {
      ...process.env,
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_CONFIG_GLOBAL: '/dev/null',
    },
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'octocode-clone-production-'));
  state.home = join(root, 'home');
  state.remote = join(root, 'remote');
  state.failPromotion = false;
  state.calls = [];
  mkdirSync(state.remote);
  git('init', '-b', 'main');
  git('config', 'uploadpack.allowFilter', 'true');
  git('config', 'user.name', 'Fixture');
  git('config', 'user.email', 'fixture@example.test');
  mkdirSync(join(state.remote, 'src'));
  writeFileSync(join(state.remote, 'README.md'), 'first\n');
  writeFileSync(
    join(state.remote, 'src', 'target.ts'),
    'export const target = 1;\n'
  );
  git('add', '.');
  git('commit', '-m', 'first');
  sha = git('rev-parse', 'HEAD');
  git('tag', 'v1');
  writeFileSync(join(state.remote, 'README.md'), 'second\n');
  git('commit', '-am', 'second');
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe('clone production identity and atomic replacement', () => {
  it.each([undefined, 'src', 'src/target.ts'])(
    'checks out an exact SHA with sparsePath=%s',
    async sparsePath => {
      const result = await cloneRepo(
        {
          owner: 'acme',
          repo: 'fixture',
          branch: sha,
          ...(sparsePath ? { sparsePath } : {}),
        },
        undefined,
        'test-token'
      );
      expect(readFileSync(join(result.localPath, 'README.md'), 'utf8')).toBe(
        'first\n'
      );
      expect(result).toMatchObject({
        commitSha: sha,
        verified: true,
        cached: false,
      });
      expect(readCacheMeta(result.localPath)?.commitSha).toBe(sha);
      expect(state.calls.some(args => args.includes('clone'))).toBe(false);
      const fetch = state.calls.find(args => args.includes('fetch'))!;
      expect(fetch).toEqual(
        expect.arrayContaining(['--depth', '1', 'origin', sha])
      );
      expect(fetch).toContain(
        'http.https://github.com/acme/fixture.git.extraHeader=Authorization: Bearer test-token'
      );
      for (const args of state.calls.filter(
        args => args.includes('checkout') || args.includes('sparse-checkout')
      )) {
        expect(args).toContain(
          'http.https://github.com/acme/fixture.git.extraHeader=Authorization: Bearer test-token'
        );
      }
    }
  );

  it.each(['main', 'v1'])(
    'preserves branch/tag checkout and reports HEAD for %s',
    async branch => {
      const result = await cloneRepo({
        owner: 'acme',
        repo: 'fixture',
        branch,
      });
      expect(result.commitSha).toBe(git('rev-parse', branch));
      expect(result.verified).toBe(true);
      const cached = await cloneRepo({
        owner: 'acme',
        repo: 'fixture',
        branch,
      });
      expect(cached).toMatchObject({
        cached: true,
        verified: false,
        commitSha: result.commitSha,
      });
    }
  );

  it('preserves the previous checkout when replacement promotion fails', async () => {
    const query = { owner: 'acme', repo: 'fixture', branch: 'main' };
    const first = await cloneRepo(query);
    const originalMeta = readCacheMeta(first.localPath);
    writeFileSync(join(state.remote, 'README.md'), 'third\n');
    git('commit', '-am', 'third');
    state.failPromotion = true;
    await expect(cloneRepo({ ...query, forceRefresh: true })).rejects.toThrow(
      'injected promotion failure'
    );
    expect(readFileSync(join(first.localPath, 'README.md'), 'utf8')).toBe(
      'second\n'
    );
    expect(readCacheMeta(first.localPath)).toEqual(originalMeta);
  });

  it('replaces a cached checkout whose HEAD no longer matches a pinned request', async () => {
    const query = { owner: 'acme', repo: 'fixture', branch: sha };
    const first = await cloneRepo(query);
    writeFileSync(join(first.localPath, 'README.md'), 'locally changed\n');
    const changed = spawnSync(
      'git',
      [
        '-C',
        first.localPath,
        '-c',
        'user.name=Fixture',
        '-c',
        'user.email=fixture@example.test',
        'commit',
        '-am',
        'local change',
      ],
      { encoding: 'utf8' }
    );
    expect(changed.status).toBe(0);
    const recovered = await cloneRepo(query);
    expect(recovered).toMatchObject({
      commitSha: sha,
      cached: false,
      verified: true,
    });
    expect(readFileSync(join(recovered.localPath, 'README.md'), 'utf8')).toBe(
      'first\n'
    );
  });
});
