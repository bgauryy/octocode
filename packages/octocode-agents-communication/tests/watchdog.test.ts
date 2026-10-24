import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { chmodSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import type { TestContext } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { posix, tempWorkspace, waitFor } from './helpers.ts';
import { bundlePath, fixture, runtime } from './store-support.ts';

const { proxy, wire } = runtime;
type ProxyRuntime = Parameters<typeof proxy.run>[1];

/** Whether any process of the group `group` exists. */
/** `kill(pid, 0)` only tests existence: EPERM means the process exists. */
const permitted = (error: unknown) => error instanceof Error && 'code' in error && error.code === 'EPERM';
const groupAlive = (group: number) => { try { process.kill(-group, 0); return true; } catch (error) { return permitted(error); } };
const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch (error) { return permitted(error); } };


const exited = (child: ChildProcess) => new Promise<void>(resolve => (child.exitCode !== null || child.signalCode !== null ? resolve() : child.once('exit', () => resolve())));

/** A vendor script that starts a grandchild, records its pid and optionally ignores SIGTERM. */
function vendorScript(directory: string, { stubborn }: { stubborn: boolean }) {
  const script = join(directory, 'vendor.sh');
  writeFileSync(script, `#!/bin/sh\n${stubborn ? "trap '' TERM\n" : ''}sleep 100 &\necho $! > '${join(directory, 'grandchild')}'\nwhile true; do sleep 1; done\n`);
  chmodSync(script, 0o755);
  return script;
}

const grandchildOf = (directory: string) => () => existsSync(join(directory, 'grandchild')) && readFileSync(join(directory, 'grandchild'), 'utf8').trim() !== '';

/**
 * A worker process: it spawns the vendor like `proxy.worker` does, optionally with the watchdog, reports its pids on stdout
 * (`vendor <pid> <watchdog pid|none>`) and then idles until the test kills it.
 */
function startWorker(t: TestContext, directory: string, { guarded, stubborn }: { guarded: boolean; stubborn: boolean }) {
  const script = vendorScript(directory, { stubborn });
  const program = `import(${JSON.stringify('file://' + bundlePath)}).then(({ proxy, wire }) => {
    const host = new wire.Wire(${JSON.stringify(script)}, [], ${JSON.stringify(directory)}, [], null);
    const guard = ${guarded} ? proxy.watchdog(host.pid) : null;
    process.stdout.write('vendor ' + host.pid + ' ' + (guard ? guard.pid : 'none') + '\\n');
    setInterval(() => undefined, 1000);
  });`;
  const child = spawn(process.execPath, ['--input-type=commonjs', '-e', program], { stdio: ['ignore', 'pipe', 'inherit'] });
  const report = new Promise<{ vendor: number; watchdog: number | null }>((resolve, reject) => {
    let text = '';
    child.stdout.on('data', chunk => {
      text += chunk;
      const match = /vendor (\d+) (\d+|none)\n/.exec(text);
      if (match) resolve({ vendor: Number(match[1]), watchdog: match[2] === 'none' ? null : Number(match[2]) });
    });
    child.once('exit', () => reject(new Error('worker exited early: ' + text)));
  });
  t.after(() => child.kill('SIGKILL'));
  return { child, report };
}

/** Kills whatever the test leaves behind so a failing assertion does not leak processes. */
function reap(t: TestContext, directory: string, pids: number[]) {
  t.after(() => {
    for (const group of pids) try { process.kill(-group, 'SIGKILL'); } catch { /* gone */ }
    const file = join(directory, 'grandchild');
    if (existsSync(file)) try { process.kill(Number(readFileSync(file, 'utf8')), 'SIGKILL'); } catch { /* gone */ }
  });
}

test('premise: without the watchdog a SIGKILLed worker leaks its vendor group', { skip: !posix }, async t => {
  const directory = tempWorkspace(t, 'watchdog-', { real: true });
  const { child, report } = startWorker(t, directory, { guarded: false, stubborn: false });
  const { vendor } = await report;
  reap(t, directory, [vendor]);
  await waitFor(grandchildOf(directory), { timeout: 15000, message: 'Timed out waiting for the vendor grandchild' });
  child.kill('SIGKILL');
  await exited(child);
  // Nothing signals the group; give a would-be watchdog's two second grace time to show it is absent.
  await delay(2500);
  assert.equal(groupAlive(vendor), true, 'the orphaned vendor group is still running');
});

for (const stubborn of [false, true]) {
  test(`a SIGKILLed worker does not leak its vendor group (${stubborn ? 'vendor ignores SIGTERM: SIGKILL after the grace' : 'vendor ends on SIGTERM'})`, { skip: !posix }, async t => {
    const directory = tempWorkspace(t, 'watchdog-', { real: true });
    const { child, report } = startWorker(t, directory, { guarded: true, stubborn });
    const { vendor, watchdog } = await report;
    assert.ok(watchdog !== null, 'the guarded worker started a watchdog');
    reap(t, directory, [vendor, watchdog]);
    await waitFor(grandchildOf(directory), { timeout: 15000, message: 'Timed out waiting for the vendor grandchild' });
    const grandchild = Number(readFileSync(join(directory, 'grandchild'), 'utf8'));
    assert.ok(groupAlive(vendor) && alive(grandchild) && alive(watchdog));
    assert.notEqual(vendor, watchdog);
    child.kill('SIGKILL');
    await waitFor(() => !groupAlive(vendor) && !alive(grandchild), { timeout: 15000, message: 'Timed out waiting for the vendor group to be killed' });
    await waitFor(() => !alive(watchdog), { timeout: 15000, message: 'Timed out waiting for the watchdog to exit' });
  });
}

test('the watchdog exits without signalling anyone when the vendor group ends first', { skip: !posix }, async t => {
  const directory = tempWorkspace(t, 'watchdog-', { real: true });
  const host = new wire.Wire('sleep', ['100'], directory, [], null);
  await waitFor(() => host.pid !== undefined, { timeout: 15000, message: 'Timed out waiting for the vendor pid' });
  const guard = proxy.watchdog(host.pid);
  assert.ok(guard, 'the watchdog started');
  t.after(() => guard.kill('SIGKILL'));
  const ended = exited(guard);
  await host.close();
  await ended;
  assert.equal(guard.exitCode, 0, 'the watchdog finished by itself');
  assert.equal(proxy.watchdog(undefined), null, 'no vendor pid, nothing to guard');
});

test('a worker that ends normally stops its watchdog', { skip: !posix }, async t => {
  const directory = tempWorkspace(t, 'watchdog-', { real: true });
  const host = new wire.Wire('sleep', ['100'], directory, [], null);
  const guard = proxy.watchdog(host.pid);
  assert.ok(guard, 'the watchdog started');
  t.after(() => guard.kill('SIGKILL'));
  await host.close();
  guard.kill();
  await exited(guard);
  assert.ok(guard.pid !== undefined, 'the watchdog has a pid');
  assert.equal(alive(guard.pid), false);
});

const FAKE_CODEX = `#!/usr/bin/env node
const { spawn } = require('node:child_process');
const fs = require('node:fs');
spawn('sleep', ['100'], { stdio: 'ignore' });
fs.writeFileSync(process.env.FAKE_CODEX_PIDFILE, String(process.pid));
let buffer = '';
const send = value => process.stdout.write(JSON.stringify(value) + '\\n');
process.stdin.on('data', chunk => {
  buffer += chunk;
  for (let end = buffer.indexOf('\\n'); end >= 0; end = buffer.indexOf('\\n')) {
    const message = JSON.parse(buffer.slice(0, end));
    buffer = buffer.slice(end + 1);
    const reply = result => send({ id: message.id, result });
    if (message.method === 'config/read') reply({ config: {} });
    else if (message.method === 'skills/list') reply({ data: [] });
    else if (message.method === 'thread/start') reply({ thread: { id: 'thr_fake' } });
    else if (message.id !== undefined) reply({});
  }
});
`;

test('proxy.run starts the watchdog: a SIGKILLed codex worker leaves no vendor processes', { skip: !posix }, async t => {
  const f = fixture(t);
  const directory = tempWorkspace(t, 'watchdog-', { real: true });
  const script = join(directory, 'codex');
  writeFileSync(script, FAKE_CODEX);
  chmodSync(script, 0o755);
  const program = `import(${JSON.stringify('file://' + bundlePath)}).then(async ({ proxy, store }) => {
    const io = {
      output: value => process.stdout.write(JSON.stringify(value) + '\\n'),
      open: (path, workspace, options = {}) => new store.Store(path, workspace, options.readOnly ?? false, options.create ?? false),
      launcher: { mcp: args => ({ command: 'communication', args }), binary: 'communication', piExtension: '' },
    };
    await proxy.run({ vendor: 'codex', model: 'm', prompt: 'task', durationMs: 120000, workspace: ${JSON.stringify(f.workspace)}, database: ${JSON.stringify(f.file)} }, io);
  });`;
  const env = { ...process.env, PATH: directory + ':' + process.env.PATH, FAKE_CODEX_PIDFILE: join(directory, 'vendor.pid') };
  const child = spawn(process.execPath, ['--input-type=commonjs', '-e', program], { env, stdio: ['ignore', 'pipe', 'inherit'] });
  t.after(() => child.kill('SIGKILL'));
  let text = '';
  child.stdout.on('data', chunk => (text += chunk));
  await waitFor(() => /"type":"ready"/.test(text), { timeout: 15000, message: 'Timed out waiting for the worker to be ready' });
  const ready = text.split('\n').find(line => line.includes('"ready"'));
  assert.ok(ready, 'the worker reported ready');
  const vendor = JSON.parse(ready).pid;
  reap(t, directory, [vendor]);
  assert.equal(Number(readFileSync(join(directory, 'vendor.pid'), 'utf8')), vendor);
  assert.equal(groupAlive(vendor), true);
  child.kill('SIGKILL');
  await waitFor(() => !groupAlive(vendor), { timeout: 15000, message: 'Timed out waiting for the vendor group to be killed' });
});

test('parentGone follows the original parent', () => {
  assert.equal(proxy.parentGone(process.platform === 'win32' ? process.pid : process.ppid), false);
  assert.equal(proxy.parentGone(process.pid + 1000000), true);
});

/** A vendor that only sleeps: `proxy.worker` spawns it, then the injected seam decides what happens next. */
function sleepingVendor(t: TestContext) {
  const directory = tempWorkspace(t, 'watchdog-', { real: true });
  writeFileSync(join(directory, 'codex'), '#!/bin/sh\nsleep 100\n');
  chmodSync(join(directory, 'codex'), 0o755);
  const saved = process.env.PATH;
  process.env.PATH = directory + ':' + saved;
  t.after(() => { process.env.PATH = saved; });
}

const seamRuntime = (watchdog: ProxyRuntime['watchdog']): ProxyRuntime => ({
  output: () => undefined,
  open: (path, workspace, options = {}) => new runtime.store.Store(path, workspace, options.readOnly ?? false, options.create ?? false),
  launcher: { mcp: args => ({ command: 'communication', args: [...args] }), binary: 'communication', piExtension: '' },
  watchdog,
});

test('the watchdog seam runs after the vendor spawned, with its group; a seam failure still ends the vendor group', { skip: !posix }, async t => {
  const f = fixture(t);
  sleepingVendor(t);
  const seen: { group: number; aliveAtCall: boolean }[] = [];
  const failing = (group: number | undefined): never => {
    assert.ok(group !== undefined, 'the vendor has a process group');
    seen.push({ group, aliveAtCall: groupAlive(group) });
    throw new Error('watchdog could not start');
  };
  await assert.rejects(proxy.run({ vendor: 'codex', model: 'm', prompt: 'task', durationMs: 5000, workspace: f.workspace, database: f.file }, seamRuntime(failing)), /watchdog could not start/);
  assert.equal(seen.length, 1);
  assert.ok(Number.isInteger(seen[0].group));
  assert.equal(seen[0].aliveAtCall, true, 'the vendor exists before the watchdog is asked to guard it: the residual window');
  await waitFor(() => !groupAlive(seen[0].group), { timeout: 15000, message: 'Timed out waiting for the vendor group to be gone' });
});
