import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'vitest';
import { checkpointStoreDir, initCheckpointStore } from '../src/tools/checkpoints.js';

// ─── Real-git helpers ────────────────────────────────────────────────────────

function git(cwd: string, ...args: string[]): string {
  return execFileSync('git', args, {
    cwd,
    encoding: 'utf8',
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: os.devNull,
      GIT_CONFIG_SYSTEM: os.devNull,
      GIT_AUTHOR_NAME: 'user',
      GIT_AUTHOR_EMAIL: 'user@test',
      GIT_COMMITTER_NAME: 'user',
      GIT_COMMITTER_EMAIL: 'user@test',
    },
  });
}

function tmp(prefix: string): string {
  return fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), prefix));
}

function write(cwd: string, rel: string, content: string): void {
  fs.writeFileSync(path.join(cwd, rel), content);
}

function read(cwd: string, rel: string): string {
  return fs.readFileSync(path.join(cwd, rel), 'utf8');
}

// ─── #1 invariant: the user's repo is NEVER touched ─────────────────────────

test('snapshot + restore never touch the user repo index, HEAD, or status', async () => {
  const cwd = tmp('ock-user-');
  git(cwd, 'init', '-q', '-b', 'main');
  write(cwd, 'a.txt', 'v1\n');
  git(cwd, 'add', 'a.txt');
  git(cwd, 'commit', '-q', '-m', 'c1');
  write(cwd, 'a.txt', 'v2\n');
  git(cwd, 'add', 'a.txt'); // staged, uncommitted change
  write(cwd, 'b.txt', 'untracked\n');

  const userState = () => ({
    status: git(cwd, 'status', '--porcelain'),
    head: git(cwd, 'rev-parse', 'HEAD'),
    index: git(cwd, 'ls-files', '--stage'),
  });
  const before = userState();

  const home = tmp('ock-home-');
  const engine = await initCheckpointStore(cwd, home);
  const s1 = await engine.snapshot('first');
  assert.ok(s1, 'snapshot returns a result');
  assert.ok(s1.filesChanged >= 2, 'both files land in the shadow snapshot');

  write(cwd, 'a.txt', 'v3\n');
  const s2 = await engine.snapshot('second', { entryId: 'entry-42' });
  assert.ok(s2 && s2.id !== s1.id);

  await engine.restoreFiles(s1.id);
  assert.equal(read(cwd, 'a.txt'), 'v2\n', 'work tree content restored');
  assert.equal(read(cwd, 'b.txt'), 'untracked\n');

  // User repo state (status, HEAD, index blobs) is byte-identical.
  assert.deepEqual(userState(), before);

  // Shadow store lives under home, never inside the user repo.
  const store = checkpointStoreDir(cwd, home);
  assert.ok(store.startsWith(home));
  assert.ok(fs.existsSync(path.join(store, 'repo.git', 'HEAD')));
  assert.ok(fs.existsSync(path.join(store, 'index')), 'private shadow index exists');
  assert.ok(!fs.existsSync(path.join(cwd, '.git', 'shallow')), 'no shadow artifacts in user .git');
});

// ─── Round-trip + listing in a plain (non-git) directory ────────────────────

test('snapshot -> modify -> restore round-trips content; list parses labels and entry ids', async () => {
  const cwd = tmp('ock-plain-');
  const home = tmp('ock-home-');
  write(cwd, 'a.txt', 'one\n');
  write(cwd, 'b.txt', 'b1\n');

  const engine = await initCheckpointStore(cwd, home);
  const s1 = await engine.snapshot('first', { entryId: 'entry-1' });
  assert.ok(s1);
  assert.equal(s1.filesChanged, 2);

  write(cwd, 'a.txt', 'two\n');
  write(cwd, 'b.txt', 'b2\n');
  const s2 = await engine.snapshot('second');
  assert.ok(s2);

  // diffStat compares a checkpoint against the current work tree.
  write(cwd, 'a.txt', 'three\n');
  const diff = await engine.diffStat(s1.id);
  assert.ok(diff.some((e) => e.path === 'a.txt' && e.status === 'M'));

  // Partial restore: only a.txt, b.txt keeps its current content.
  await engine.restoreFiles(s1.id, ['a.txt']);
  assert.equal(read(cwd, 'a.txt'), 'one\n');
  assert.equal(read(cwd, 'b.txt'), 'b2\n');

  // Full restore of the newer checkpoint.
  await engine.restoreFiles(s2.id);
  assert.equal(read(cwd, 'a.txt'), 'two\n');
  assert.equal(read(cwd, 'b.txt'), 'b2\n');

  const list = await engine.listCheckpoints();
  assert.equal(list.length, 2);
  assert.equal(list[0]!.label, 'second');
  assert.equal(list[0]!.entryId, undefined);
  assert.equal(list[1]!.label, 'first');
  assert.equal(list[1]!.entryId, 'entry-1');
  assert.equal(list[1]!.filesChanged, 2);
  assert.ok(list[0]!.ts > 0);
});

test('init is idempotent — a second engine sees the first engine history', async () => {
  const cwd = tmp('ock-idem-');
  const home = tmp('ock-home-');
  write(cwd, 'f.txt', '1\n');

  const first = await initCheckpointStore(cwd, home);
  assert.ok(await first.snapshot('one'));

  const second = await initCheckpointStore(cwd, home);
  write(cwd, 'f.txt', '2\n');
  assert.ok(await second.snapshot('two'));

  const labels = (await second.listCheckpoints()).map((c) => c.label);
  assert.deepEqual(labels, ['two', 'one']);
});

test('restoreFiles rejects non-commitish ids (no argv injection)', async () => {
  const cwd = tmp('ock-guard-');
  const home = tmp('ock-home-');
  write(cwd, 'f.txt', 'x\n');
  const engine = await initCheckpointStore(cwd, home);
  await engine.snapshot('one');
  await assert.rejects(engine.restoreFiles('--help'));
  await assert.rejects(engine.diffStat('HEAD; rm -rf /'));
});

// ─── prune ───────────────────────────────────────────────────────────────────

test('prune keeps the newest N checkpoints', async () => {
  const cwd = tmp('ock-prune-');
  const home = tmp('ock-home-');
  const engine = await initCheckpointStore(cwd, home);
  for (let i = 1; i <= 5; i++) {
    write(cwd, 'f.txt', `v${i}\n`);
    assert.ok(await engine.snapshot(`snap ${i}`));
  }
  await engine.prune(3);
  const labels = (await engine.listCheckpoints()).map((c) => c.label);
  assert.deepEqual(labels, ['snap 5', 'snap 4', 'snap 3']);

  // Pruned history still snapshots and restores fine.
  write(cwd, 'f.txt', 'v6\n');
  const s6 = await engine.snapshot('snap 6');
  assert.ok(s6);
  assert.equal((await engine.listCheckpoints())[0]!.label, 'snap 6');
});

test('prune is a no-op when at or under the keep limit', async () => {
  const cwd = tmp('ock-prune2-');
  const home = tmp('ock-home-');
  const engine = await initCheckpointStore(cwd, home);
  write(cwd, 'f.txt', '1\n');
  await engine.snapshot('only');
  await engine.prune(30);
  assert.equal((await engine.listCheckpoints()).length, 1);
});

// ─── Latency guard ───────────────────────────────────────────────────────────

test('latency guard self-disables the engine when a snapshot exceeds the budget', async () => {
  const cwd = tmp('ock-slow-');
  const home = tmp('ock-home-');
  write(cwd, 'f.txt', 'x\n');
  // Guard of 0ms: any real git invocation blows the budget.
  const engine = await initCheckpointStore(cwd, home, { latencyGuardMs: 0 });
  assert.equal(engine.isDisabled(), false);
  assert.equal(await engine.snapshot('too slow'), undefined);
  assert.equal(engine.isDisabled(), true);
  // Disabled engines short-circuit — no more snapshots for this session.
  assert.equal(await engine.snapshot('after disable'), undefined);
});
