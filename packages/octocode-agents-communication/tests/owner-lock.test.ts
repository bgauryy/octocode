import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import type { TestContext } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { posix, tempWorkspace, waitFor } from './helpers.ts';
import { bundlePath, runtime } from './store-support.ts';

const { ownerLock } = runtime;

/** A separate process that claims the lock, reports `claimed <address>` and holds it until killed. */
function holder(t: TestContext, database: string, session: string) {
  const program = `import(${JSON.stringify('file://' + bundlePath)}).then(async ({ ownerLock }) => {
    const lock = await ownerLock.claimDeliveryOwner(${JSON.stringify(database)}, ${JSON.stringify(session)});
    process.stdout.write('claimed ' + lock.address + '\\n');
    setInterval(() => undefined, 1000);
  }).catch(error => { process.stdout.write('failed ' + error.message + '\\n'); process.exit(3); });`;
  const child = spawn(process.execPath, ['--input-type=commonjs', '-e', program], { stdio: ['ignore', 'pipe', 'inherit'] });
  t.after(() => child.kill('SIGKILL'));
  const exited = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(resolve => child.once('exit', (code, signal) => resolve({ code, signal })));
  const claimed = new Promise<string>((resolve, reject) => {
    let text = '';
    child.stdout.on('data', chunk => {
      text += chunk;
      if (text.includes('\n')) text.startsWith('claimed ') ? resolve(text.trim().slice(8)) : reject(new Error(text));
    });
    exited.then(() => reject(new Error('holder exited: ' + text)));
  });
  return { child, claimed, exited };
}

test('acquiring the lock makes the owner live and release removes it', async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
  assert.equal(ownerLock.deliveryOwnerLive(database, 's2'), false, 'another session has its own lock');
  if (posix) assert.ok(statSync(lock.address).isSocket());
  await lock.release();
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
  if (posix) assert.equal(existsSync(lock.address), false);
  await lock.release();
});

test('a second claim in the same process is refused with the owner message', async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  t.after(() => lock.release());
  await assert.rejects(ownerLock.claimDeliveryOwner(database, 's1'), (error: unknown) => error instanceof Error && error.message === ownerLock.OWNED_ELSEWHERE);
  const other = await ownerLock.claimDeliveryOwner(database, 's2');
  await other.release();
});

test('the lock is held across processes and released by SIGKILL; the stale socket is recovered', { skip: !posix }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const first = holder(t, database, 's1');
  const address = await first.claimed;
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
  await assert.rejects(ownerLock.claimDeliveryOwner(database, 's1'), (error: unknown) => error instanceof Error && error.message === ownerLock.OWNED_ELSEWHERE);
  const second = holder(t, database, 's1');
  await assert.rejects(second.claimed, /Another delivery owner/);
  assert.equal((await second.exited).code, 3);

  first.child.kill('SIGKILL');
  await first.exited;
  assert.equal(existsSync(address), true, 'SIGKILL leaves the socket file behind');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false, 'nothing listens, so the owner is dead');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
  await lock.release();
});

test('a SIGTERM-killed owner is not live and its address is reusable', { skip: !posix }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const owner = holder(t, database, 's1');
  const address = await owner.claimed;
  owner.child.kill('SIGTERM');
  await owner.exited;
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
  // SIGTERM without a handler skips exit handlers; the next claim recovers or reuses the address.
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  await lock.release();
  assert.equal(existsSync(address), false);
});

test('a database path too long for a socket address still locks', { skip: !posix }, async t => {
  let path = tempWorkspace(t, 'owner-lock-', { real: true });
  for (const part of ['a'.repeat(40), 'b'.repeat(40), 'c'.repeat(40)]) {
    path = join(path, part);
    mkdirSync(path);
  }
  const database = join(path, 'db.sqlite');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  assert.ok(Buffer.byteLength(lock.address) <= 103, lock.address);
  assert.notEqual(lock.address, ownerLock.ownerLockPath(database, 's1'));
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
  await assert.rejects(ownerLock.claimDeliveryOwner(database, 's1'), /Another delivery owner/);
  await lock.release();
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
});

test('the lock disposes asynchronously', async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  let address = '';
  {
    await using lock = await ownerLock.claimDeliveryOwner(database, 's1');
    assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
    if (posix) assert.equal(existsSync(lock.address), true, 'the held lock listens on its socket');
    address = lock.address;
  }
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
  if (posix) assert.equal(existsSync(address), false, 'disposal removes the socket');
});

const sidecarOf = (address: string) => address + '.pid';

test('the owner records pid, start time and socket inode beside the socket and removes them on release', { skip: !posix }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  const identity = JSON.parse(readFileSync(sidecarOf(lock.address), 'utf8'));
  assert.equal(identity.pid, process.pid);
  assert.equal(identity.ino, String(lstatSync(lock.address, { bigint: true }).ino));
  assert.ok(identity.start === null || typeof identity.start === 'string');
  assert.equal(ownerLock.ownedHere(database, 's1'), true);
  assert.equal(ownerLock.ownedHere(database, 's2'), false);
  await lock.release();
  assert.equal(existsSync(sidecarOf(lock.address)), false);
  assert.equal(ownerLock.ownedHere(database, 's1'), false);
});

test('a SIGSTOPped owner stays live over 200 probes, then reads dead after SIGKILL', { skip: !posix, timeout: 120000 }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const owner = holder(t, database, 's1');
  await owner.claimed;
  owner.child.kill('SIGSTOP');
  t.after(() => owner.child.kill('SIGCONT'));
  for (let probe = 1; probe <= 200; probe += 1) assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true, `probe ${probe}`);
  await assert.rejects(ownerLock.claimDeliveryOwner(database, 's1'), (error: unknown) => error instanceof Error && error.message === ownerLock.OWNED_ELSEWHERE, 'a frozen owner still holds the lock');
  owner.child.kill('SIGKILL');
  await owner.exited;
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);
});

test('a sidecar that does not describe the live process or socket reads dead', { skip: !posix, timeout: 60000 }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const owner = holder(t, database, 's1');
  const address = await owner.claimed;
  const sidecar = sidecarOf(address);
  const recorded = JSON.parse(readFileSync(sidecar, 'utf8'));
  owner.child.kill('SIGKILL');
  await owner.exited;

  // The pid exists again under another process (reuse): the start time differs, so it is not the owner.
  writeFileSync(sidecar, JSON.stringify({ pid: process.pid, start: recorded.start === null ? null : 'not the owner start', ino: recorded.ino }));
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), recorded.start === null, 'only a recorded start time can tell reuse apart');

  // The sidecar names a live pid but another socket: unproven while the socket is young (an owner may be mid-claim) ...
  writeFileSync(sidecar, JSON.stringify({ pid: process.pid, start: null, ino: '1' }));
  const age = Date.now() - Number(lstatSync(address, { bigint: true }).ctimeMs);
  if (age < 900) assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true, 'a young socket without its sidecar is not an orphan');
  // ... and dead once it is older than the grace.
  await delay(1100);
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false);

  writeFileSync(sidecar, 'not json');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false, 'an unreadable sidecar on an old socket');
  rmSync(sidecar);
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false, 'no sidecar on an old socket');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
  await lock.release();
});

test('a probe is not affected by an earlier probe: no state survives between calls', { skip: !posix }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  for (let i = 0; i < 3; i += 1) {
    assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true);
    assert.equal(ownerLock.deliveryOwnerLive(database, 'absent'), false);
  }
  await lock.release();
});

/** A process that is dead but not reaped: its parent (`exec sleep`) never waits for it. */
async function zombie(t: TestContext) {
  const parent = spawn('sh', ['-c', 'sleep 300 & echo $!; exec sleep 300'], { stdio: ['ignore', 'pipe', 'inherit'] });
  t.after(() => parent.kill('SIGKILL'));
  const pid = await new Promise<number>((resolve, reject) => {
    parent.stdout.once('data', chunk => resolve(Number(String(chunk).trim())));
    parent.once('exit', () => reject(new Error('parent exited')));
  });
  const state = () => spawnSync('ps', ['-o', 'stat=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim();
  assert.ok(state().length > 0, 'the child runs');
  return { pid, kill: async () => { process.kill(pid, 'SIGKILL'); await waitFor(() => state().startsWith('Z'), { message: 'The killed child did not stay a zombie' }); } };
}

test('an owner that died but is not reaped (zombie) reads dead, a running one reads live', { skip: !posix, timeout: 60000 }, async t => {
  const database = join(tempWorkspace(t, 'owner-lock-', { real: true }), 'db.sqlite');
  const lock = await ownerLock.claimDeliveryOwner(database, 's1');
  t.after(() => lock.release());
  const sidecar = lock.address + '.pid';
  const recorded = JSON.parse(readFileSync(sidecar, 'utf8'));
  const child = await zombie(t);
  writeFileSync(sidecar, JSON.stringify({ pid: child.pid, start: null, ino: recorded.ino }));
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), true, 'the running child holds the socket');
  await child.kill();
  assert.equal(ownerLock.deliveryOwnerLive(database, 's1'), false, 'a zombie holds nothing');
  assert.equal(await ownerLock.claimDeliveryOwner(database, 's2').then(owner => owner.release().then(() => true)), true);
});
