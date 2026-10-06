import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiClient } from '../src/api/client.js';
import { registerApi } from '../src/api/register.js';
import { API_DIR_ENV, apiDir, instancesDir, listInstances, socketPath, writeRecord, type InstanceRecord } from '../src/api/registry.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const stops: Array<() => Promise<void>> = [];
afterEach(async () => {
  await Promise.all(stops.splice(0).map((stop) => stop()));
  vi.unstubAllEnvs();
});

function setup(env: Record<string, string> = {}) {
  const dir = tmp('octo-api-reg-');
  vi.stubEnv(API_DIR_ENV, dir);
  vi.stubEnv('OCTOCODE_API', '');
  vi.stubEnv('OCTOCODE_API_HTTP', '');
  for (const [name, value] of Object.entries(env)) vi.stubEnv(name, value);
  const { pi } = fakePi();
  const commands = new Subcommands();
  const wired = registerApi(pi, commands, { list: () => [], tell: () => ({ sent: [] }) }, () => '1.2.3');
  stops.push(wired.stop);
  const ctx = fakeCtx({ cwd: tmp() });
  const run = (args: string) => commands.get('api')!.handler(args, ctx);
  return { dir, wired, commands, ctx, run, notes: ctx.ui.notes };
}

const deadPid = (): number => spawnSync(process.execPath, ['-e', '0']).pid!;

describe('/octocode api', () => {
  it('reports off, starts on the socket, adds HTTP, stops again, and rejects unknown actions', async () => {
    const { run, notes, wired, dir } = setup();
    await run('');
    expect(notes.at(-1)).toMatchObject({ type: 'info', message: expect.stringContaining('Octocode API is off') });

    await run('on');
    const info = wired.api.instance!;
    expect(notes.at(-1)!.message).toContain(`Octocode API ${info.id}`);
    expect(notes.at(-1)!.message).toContain(`Discovery: ${dir}/instances/${info.id}.json`);
    if (process.platform !== 'win32') expect(await new ApiClient(info).call('ping')).toEqual({});

    await run('on http');
    const withHttp = wired.api.instance!;
    expect(withHttp.http?.url).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
    expect(notes.at(-1)!.message).toContain('(token in the instance file)');
    expect(listInstances(dir).map((record) => record.id)).toEqual([withHttp.id]);

    await run('off');
    expect(wired.api.instance).toBeUndefined();
    expect(listInstances(dir)).toEqual([]);
    expect(notes.at(-1)!.message).toContain('Octocode API is off');
    // `off` rebinds the session, so `on` works again.
    await run('on');
    expect(wired.api.running).toBe(true);

    await run('sideways');
    expect(notes.at(-1)).toEqual({ type: 'warning', message: 'Usage: /octocode api [on [http] | off]' });
    expect(setup().commands.get('api')!.complete!('o')?.map((item) => item.value)).toEqual(['on', 'on http', 'off']);
  });

  it('reports a start failure as an error instead of throwing', async () => {
    const { run, notes, dir } = setup();
    // A file where the instances directory must go makes the start fail.
    fs.writeFileSync(instancesDir(dir), 'not a directory');
    await run('on');
    expect(notes.at(-1)!.type).toBe('error');
    expect(notes.at(-1)!.message).toMatch(/EEXIST|ENOTDIR|not a directory/i);
  });

  it('starts from the environment at session start and only warns (with a UI) when that fails', async () => {
    const { wired, ctx, dir } = setup({ OCTOCODE_API: '1', OCTOCODE_API_HTTP: '0' });
    await wired.start(ctx);
    expect(wired.api.instance?.http).toBeDefined();
    await wired.stop();
    fs.rmSync(dir, { recursive: true, force: true });
    fs.writeFileSync(dir, 'blocked');
    await wired.start(ctx);
    expect(ctx.ui.notes.at(-1)?.type).toBe('warning');
    const headless = fakeCtx({ cwd: tmp(), hasUI: false });
    await wired.start(headless);
    expect(headless.ui.notes).toEqual([]);
  });
});

describe('instance registry', () => {
  const record = (id: string, extra: Partial<InstanceRecord> = {}): InstanceRecord => ({ id, pid: process.pid, cwd: '/w', startedAt: 1, protocol: 1, ...extra });

  it('resolves the directory from the override or the Octocode home', () => {
    expect(apiDir({ [API_DIR_ENV]: ' relative/api ' })).toBe(path.resolve('relative/api'));
    expect(path.isAbsolute(apiDir({}))).toBe(true);
  });

  it('lists live records oldest first, skips corrupt or partial ones and sweeps dead ones with their sockets', () => {
    const dir = tmp('octo-reg-');
    expect(listInstances(path.join(dir, 'missing'))).toEqual([]);
    writeRecord(dir, record('pi-b', { startedAt: 20 }));
    writeRecord(dir, record('pi-a', { startedAt: 10 }));
    const socket = path.join(dir, 'dead.sock');
    fs.writeFileSync(socket, '');
    writeRecord(dir, record('pi-dead', { pid: deadPid(), socket }));
    fs.writeFileSync(path.join(instancesDir(dir), 'corrupt.json'), '{');
    fs.writeFileSync(path.join(instancesDir(dir), 'partial.json'), JSON.stringify({ id: 7 }));
    fs.writeFileSync(path.join(instancesDir(dir), 'notes.txt'), 'ignored');
    expect(listInstances(dir).map((entry) => entry.id)).toEqual(['pi-a', 'pi-b']);
    expect(fs.existsSync(path.join(instancesDir(dir), 'pi-dead.json'))).toBe(false);
    expect(fs.existsSync(socket)).toBe(false);
    // Corrupt files are left for their writer, not deleted.
    expect(fs.existsSync(path.join(instancesDir(dir), 'corrupt.json'))).toBe(true);
    if (process.platform !== 'win32') expect(fs.statSync(instancesDir(dir)).mode & 0o777).toBe(0o700);
  });

  it('keeps a socket next to its record when the path fits and moves a long one to the temp directory', () => {
    expect(socketPath('/short', 'pi-1')).toBe(path.join('/short', 'pi-1.sock'));
    const long = socketPath(`/${'x'.repeat(120)}`, 'pi-2');
    expect(path.basename(long)).toMatch(/^octocode-\d+-pi-2\.sock$/);
    expect(Buffer.byteLength(long)).toBeLessThan(110);
  });
});
