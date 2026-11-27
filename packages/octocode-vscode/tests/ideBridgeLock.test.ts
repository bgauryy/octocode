import * as fs from 'fs/promises';
import * as http from 'http';
import * as path from 'path';
import { describe, expect, it, vi } from 'vitest';

vi.mock('vscode', async () => (await import('./ideBridgeMock')).vscodeMock());

import { IdeBridge } from '../src/ideBridge';
import { ctx, lock, posix, send, useBridge } from './ideBridgeHarness';
import { host } from './ideBridgeMock';

useBridge();

describe('lock file contents', () => {
  it('publishes pid, ide, port, token and workspace folders', async () => {
    host.folders = [
      { uri: { scheme: 'file', fsPath: '/work' } },
      { uri: { scheme: 'vscode-remote', fsPath: '/remote' } },
    ];
    await ctx.bridge.start();
    const content = await lock();
    expect(content).toEqual({
      version: 1,
      pid: process.pid,
      ide: 'Test IDE',
      port: expect.any(Number),
      token: expect.stringMatching(/^[0-9a-f]{64}$/),
      workspaceFolders: ['/work'],
    });
    expect(content.port).toBeGreaterThan(0);
    expect(ctx.log.join('\n')).toContain(`127.0.0.1:${content.port}`);
    expect(ctx.log.join('\n')).not.toContain(content.token);
  });

  it.runIf(posix)('is owner-only and leaves no temp file', async () => {
    await ctx.bridge.start();
    const directory = path.join(ctx.home, 'ide');
    expect((await fs.stat(directory)).mode & 0o777).toBe(0o700);
    const file = path.join(directory, `${process.pid}.json`);
    expect((await fs.stat(file)).mode & 0o777).toBe(0o600);
    expect(await fs.readdir(directory)).toEqual([`${process.pid}.json`]);
  });

  it.runIf(posix)('tightens an existing directory to 0700', async () => {
    await fs.mkdir(path.join(ctx.home, 'ide'), { mode: 0o755 });
    await ctx.bridge.start();
    expect((await fs.stat(path.join(ctx.home, 'ide'))).mode & 0o777).toBe(
      0o700
    );
  });

  it('uses a fresh token for every start', async () => {
    await ctx.bridge.start();
    const first = (await lock()).token;
    await ctx.bridge.stop();
    await ctx.bridge.start();
    expect((await lock()).token).not.toBe(first);
  });
});

describe('lock file lifecycle', () => {
  it('does not resurrect the lock when stopped during a rewrite', async () => {
    await ctx.bridge.start();
    host.foldersChanged!();
    await ctx.bridge.stop();
    expect(await fs.readdir(path.join(ctx.home, 'ide'))).toEqual([]);
  });

  it('cleans up its temp file when the rewrite fails', async () => {
    await ctx.bridge.start();
    const file = path.join(ctx.home, 'ide', `${process.pid}.json`);
    await fs.rm(file);
    await fs.mkdir(path.join(file, 'blocker'), { recursive: true });
    host.foldersChanged!();
    await vi.waitFor(() =>
      expect(ctx.log.join('\n')).toContain('lock file update failed')
    );
    expect(await fs.readdir(path.join(ctx.home, 'ide'))).toEqual([
      `${process.pid}.json`,
    ]);
    await fs.rm(file, { recursive: true });
  });

  it('rewrites the lock when workspace folders change', async () => {
    await ctx.bridge.start();
    host.folders = [{ uri: { scheme: 'file', fsPath: '/other' } }];
    host.foldersChanged!();
    await vi.waitFor(async () =>
      expect((await lock()).workspaceFolders).toEqual(['/other'])
    );
    expect(await fs.readdir(path.join(ctx.home, 'ide'))).toEqual([
      `${process.pid}.json`,
    ]);
  });

  it('logs a failed rewrite', async () => {
    await ctx.bridge.start();
    await fs.rm(path.join(ctx.home, 'ide'), { recursive: true });
    await fs.writeFile(path.join(ctx.home, 'ide'), 'blocked');
    host.foldersChanged!();
    await vi.waitFor(() =>
      expect(ctx.log.join('\n')).toContain('lock file update failed')
    );
  });

  it('is removed on stop and dispose, and stop is idempotent', async () => {
    const file = path.join(ctx.home, 'ide', `${process.pid}.json`);
    await ctx.bridge.start();
    const { port } = await lock();
    await ctx.bridge.stop();
    await expect(fs.stat(file)).rejects.toMatchObject({ code: 'ENOENT' });
    expect(host.foldersDispose).toHaveBeenCalledTimes(1);
    await expect(send(port, {})).rejects.toMatchObject({
      code: 'ECONNREFUSED',
    });
    await ctx.bridge.stop();
    expect(ctx.bridge.running).toBe(false);

    await ctx.bridge.start();
    await fs.stat(file);
    ctx.bridge.dispose();
    await ctx.bridge.stop();
    await expect(fs.stat(file)).rejects.toMatchObject({ code: 'ENOENT' });
  });

  it('start is idempotent', async () => {
    await ctx.bridge.start();
    const first = await lock();
    await ctx.bridge.start();
    expect(await lock()).toEqual(first);
  });
});

describe('lock file failures', () => {
  it('reports a lock that cannot be removed', async () => {
    await ctx.bridge.start();
    const file = path.join(ctx.home, 'ide', `${process.pid}.json`);
    await fs.rm(file);
    await ctx.bridge.stop(); // already gone: not an error
    expect(ctx.log.join('\n')).not.toContain('could not remove');
    await ctx.bridge.start();
    await fs.rm(file);
    await fs.mkdir(file);
    await ctx.bridge.stop();
    expect(ctx.log.join('\n')).toContain('could not remove');
    await fs.rm(file, { recursive: true });
  });

  it('closes the server when the lock cannot be written', async () => {
    await fs.writeFile(path.join(ctx.home, 'ide'), 'blocked');
    await expect(ctx.bridge.start()).rejects.toThrow();
    expect(ctx.bridge.running).toBe(false);
  });

  it('reports a port that cannot be bound', async () => {
    vi.spyOn(http.Server.prototype, 'listen').mockImplementation(function (
      this: http.Server
    ) {
      queueMicrotask(() => this.emit('error', new Error('EADDRINUSE')));
      return this;
    } as never);
    await expect(ctx.bridge.start()).rejects.toThrow('EADDRINUSE');
    expect(ctx.bridge.running).toBe(false);
  });
});

describe('octocode home', () => {
  it('defaults the lock directory to the Octocode home', async () => {
    vi.stubEnv('OCTOCODE_HOME', ctx.home);
    const defaulted = new IdeBridge({ output: line => ctx.log.push(line) });
    await defaulted.start();
    try {
      await fs.stat(path.join(ctx.home, 'ide', `${process.pid}.json`));
    } finally {
      await defaulted.stop();
      vi.unstubAllEnvs();
    }
  });
});

describe.runIf(posix)('stale lock pruning', () => {
  const dead = () => {
    // A pid that is not alive: a reaped child's.
    for (let pid = 2_000_000; ; pid += 1) {
      try {
        process.kill(pid, 0);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ESRCH') return pid;
      }
    }
  };

  it('removes dead pid locks at start and leaves everything else', async () => {
    const directory = path.join(ctx.home, 'ide');
    await fs.mkdir(directory, { recursive: true });
    const stale = path.join(directory, `${dead()}.json`);
    const alive = path.join(directory, `${process.ppid}.json`);
    const other = path.join(directory, 'notes.json');
    const padded = path.join(directory, `${dead()}.json.bak`);
    const subdir = path.join(directory, `${dead() + 1}.json`);
    const link = path.join(directory, `${dead() + 2}.json`);
    for (const file of [stale, alive, other, padded]) {
      await fs.writeFile(file, '{}');
    }
    await fs.mkdir(subdir);
    await fs.symlink(other, link);
    await ctx.bridge.start();
    const names = (await fs.readdir(directory)).sort();
    expect(names).toEqual(
      [
        path.basename(alive),
        path.basename(link),
        path.basename(other),
        path.basename(padded),
        path.basename(subdir),
        `${process.pid}.json`,
      ].sort()
    );
  });

  it('keeps a lock whose pid exists but is not signalable (EPERM)', async () => {
    const directory = path.join(ctx.home, 'ide');
    await fs.mkdir(directory, { recursive: true });
    const file = path.join(directory, '4242424.json');
    await fs.writeFile(file, '{}');
    const kill = vi.spyOn(process, 'kill').mockImplementation(() => {
      throw Object.assign(new Error('nope'), { code: 'EPERM' });
    });
    await ctx.bridge.start();
    kill.mockRestore();
    await fs.stat(file);
  });

  it('keeps locks owned by another user', async () => {
    const directory = path.join(ctx.home, 'ide');
    await fs.mkdir(directory, { recursive: true });
    const file = path.join(directory, `${dead()}.json`);
    await fs.writeFile(file, '{}');
    vi.spyOn(process, 'getuid').mockReturnValue(process.getuid!() + 1);
    await ctx.bridge.start();
    await fs.stat(file);
  });

  it('pruning a non-directory does not mask the start error', async () => {
    await fs.writeFile(path.join(ctx.home, 'ide'), 'a file, not a directory');
    await expect(ctx.bridge.start()).rejects.toThrow();
  });
});
