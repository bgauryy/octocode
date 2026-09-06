import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  mkdtempSync,
  readFileSync,
  rmSync,
  existsSync,
  writeFileSync,
  symlinkSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const state = vi.hoisted(() => ({
  home: '',
  failWrites: false,
  failPublish: false,
  getContent: vi.fn(),
  getCommit: vi.fn(),
  defaultBranch: vi.fn(),
}));
vi.mock('node:fs', async original => {
  const fs = await original<typeof import('node:fs')>();
  return {
    ...fs,
    renameSync: (...args: Parameters<typeof fs.renameSync>) => {
      if (
        state.failPublish &&
        String(args[1]).endsWith('.octocode-clone-meta.json')
      )
        throw new Error('injected publish failure');
      return fs.renameSync(...args);
    },
    writeFileSync: (...args: Parameters<typeof fs.writeFileSync>) => {
      if (state.failWrites && String(args[0]).endsWith('b.ts'))
        throw new Error('injected disk failure');
      return fs.writeFileSync(...args);
    },
  };
});
vi.mock('../../src/shared/paths.js', async original => ({
  ...(await original<typeof import('../../src/shared/paths.js')>()),
  getOctocodeDir: () => state.home,
}));
vi.mock('../../src/github/client.js', () => ({
  getOctokit: async () => ({
    rest: {
      repos: { getContent: state.getContent, getCommit: state.getCommit },
    },
  }),
  resolveDefaultBranch: state.defaultBranch,
  resolveCacheAuthFingerprint: async () => 'fixture-enterprise-auth',
}));
const { fetchDirectoryContents } =
  await import('../../src/github/directoryFetch/fetchDirectoryContents.js');
const { fetchFileContentToDisk } =
  await import('../../src/github/directoryFetch/fetchFileContentToDisk.js');
const { resolveMaterializationRef } =
  await import('../../src/github/directoryFetch/refResolution.js');
const { clearAllCache } =
  await import('../../src/utils/http/cache/management.js');
const SHA = 'a'.repeat(40);
const body = 'export const value = 1;\n';
function entry(
  path: string,
  downloadUrl:
    string | null = `https://raw.githubusercontent.com/o/r/${SHA}/${path}`
) {
  return {
    name: path.split('/').at(-1),
    path,
    type: 'file',
    size: body.length,
    download_url: downloadUrl,
  };
}
function serve(entries = [entry('src/a.ts'), entry('src/b.ts')]) {
  state.getContent.mockImplementation(async ({ path }: { path: string }) => {
    const file = entries.find(e => e.path === path);
    if (file)
      return {
        data: {
          ...file,
          content: Buffer.from(body).toString('base64'),
          encoding: 'base64',
        },
        headers: {},
      };
    return {
      data:
        path === ''
          ? [
              {
                name: 'src',
                path: 'src',
                type: 'dir',
                size: 0,
                download_url: null,
              },
            ]
          : entries,
      headers: {},
    };
  });
}
describe('production materialization invariants', () => {
  beforeEach(() => {
    state.home = mkdtempSync(
      join(tmpdir(), 'octocode-materialization-production-')
    );
    state.failWrites = false;
    state.failPublish = false;
    state.getContent.mockReset();
    state.getCommit.mockReset().mockResolvedValue({ data: { sha: SHA } });
    state.defaultBranch.mockReset().mockResolvedValue('trunk');
    clearAllCache();
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => ({ ok: true, text: async () => body }))
    );
    serve();
  });
  afterEach(() => {
    state.failWrites = false;
    clearAllCache();
    vi.unstubAllGlobals();
    rmSync(state.home, { recursive: true, force: true });
  });

  it('never changes an explicitly requested main branch after a provider error', async () => {
    state.getCommit.mockRejectedValueOnce(new Error('404 missing main'));
    await expect(resolveMaterializationRef('o', 'r', 'main')).rejects.toThrow(
      'missing main'
    );
    expect(state.defaultBranch).not.toHaveBeenCalled();
  });

  it.each([null, 'https://git.enterprise.test/o/r/raw/src/a.ts'])(
    'reads through the configured GitHub API when download_url is %s',
    async url => {
      serve([entry('src/a.ts', url)]);
      const result = await fetchDirectoryContents('o', 'r', 'src', 'release');
      expect(result.complete).toBe(true);
      expect(readFileSync(join(result.localPath, 'a.ts'), 'utf8')).toBe(body);
      expect(state.getContent).toHaveBeenCalledWith(
        expect.objectContaining({ path: 'src/a.ts', ref: SHA })
      );
      expect(fetch).not.toHaveBeenCalled();
    }
  );

  it('keeps the previously returned directory readable when a refreshed write fails', async () => {
    const first = await fetchDirectoryContents('o', 'r', 'src', 'release');
    state.failWrites = true;
    await expect(
      fetchDirectoryContents('o', 'r', 'src', 'release', undefined, true)
    ).rejects.toThrow('injected disk failure');
    expect(readFileSync(join(first.localPath, 'a.ts'), 'utf8')).toBe(body);
    expect(readFileSync(join(first.localPath, 'b.ts'), 'utf8')).toBe(body);
    state.failWrites = false;
    const cached = await fetchDirectoryContents('o', 'r', 'src', 'release');
    expect(cached.complete).toBe(true);
  });

  it('preserves child paths when a parent directory is materialized later', async () => {
    const child = await fetchDirectoryContents('o', 'r', 'src', 'release');
    const parent = await fetchDirectoryContents(
      'o',
      'r',
      '',
      'release',
      undefined,
      true
    );
    expect(parent.complete).toBe(false);
    expect(readFileSync(join(child.localPath, 'b.ts'), 'utf8')).toBe(body);
  });

  it('keeps the previous cache pointer when publication fails after staging', async () => {
    const first = await fetchDirectoryContents('o', 'r', 'src', 'release');
    state.failPublish = true;
    await expect(
      fetchDirectoryContents('o', 'r', 'src', 'release', undefined, true)
    ).rejects.toThrow('injected publish failure');
    state.failPublish = false;
    const cached = await fetchDirectoryContents('o', 'r', 'src', 'release');
    expect(cached.localPath).toBe(first.localPath);
    expect(cached.complete).toBe(true);
    expect(readFileSync(join(first.localPath, 'b.ts'), 'utf8')).toBe(body);
  });

  it.each(['asset.svg', 'yarn.lock'])(
    'does not misclassify text source %s as binary',
    async name => {
      serve([entry(`src/${name}`)]);
      const result = await fetchDirectoryContents('o', 'r', 'src', 'release');
      expect(result.complete).toBe(true);
      expect(readFileSync(join(result.localPath, name), 'utf8')).toBe(body);
    }
  );

  it('preserves all returned paths for concurrent overlapping requests', async () => {
    const results = await Promise.all([
      fetchDirectoryContents('o', 'r', 'src', 'release'),
      fetchFileContentToDisk('o', 'r', 'src/a.ts', 'release'),
      fetchDirectoryContents('o', 'r', '', 'release', undefined, true),
    ]);
    for (const result of results)
      expect(existsSync(result.localPath)).toBe(true);
    expect(readFileSync(results[1]!.localPath, 'utf8')).toBe(body);
  });

  it('does not mark copied unverified direct files as a complete refreshed directory', async () => {
    const first = await fetchDirectoryContents('o', 'r', 'src', 'release');
    writeFileSync(join(first.localPath, 'rogue.ts'), 'unverified local file');
    const refreshed = await fetchDirectoryContents(
      'o',
      'r',
      'src',
      'release',
      undefined,
      true
    );
    expect(refreshed.complete).toBe(true);
    expect(refreshed.verified).toBe(true);
    expect(existsSync(join(refreshed.localPath, 'rogue.ts'))).toBe(false);
    expect(existsSync(join(first.localPath, 'rogue.ts'))).toBe(true);
    const cached = await fetchDirectoryContents('o', 'r', 'src', 'release');
    expect(cached.complete).toBe(true);
  });

  it('replaces a dangling copied file symlink without writing outside the snapshot', async () => {
    const first = await fetchFileContentToDisk('o', 'r', 'src/a.ts', 'release');
    const outside = join(state.home, 'outside.ts');
    rmSync(first.localPath);
    symlinkSync(outside, first.localPath);
    const refreshed = await fetchFileContentToDisk(
      'o',
      'r',
      'src/a.ts',
      'release',
      undefined,
      true
    );
    expect(readFileSync(refreshed.localPath, 'utf8')).toBe(body);
    expect(existsSync(outside)).toBe(false);
  });
});
