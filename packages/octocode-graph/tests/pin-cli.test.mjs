// The frozen-executable cache prunes images nobody used within the retention period, and never the image it returns
// or one a live process pinned. Times come from an injected clock and set mtimes, never from waiting.
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readdirSync, readFileSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import test from 'node:test';
import { freezeExecutable, IMAGE_RETENTION_MS } from '../src/runtime/lib/pin-cli.ts';
import { tempDir } from './test-tmp.mjs';

const HOUR = 3_600_000;
const T0 = Date.UTC(2026, 0, 1);
const nobody = () => false;
const digestsNaming = (cache, image) =>
  readdirSync(join(cache, 'digests')).filter(name => readFileSync(join(cache, 'digests', name), 'utf8') === basename(image));
const setTime = (path, ms) => utimesSync(path, ms / 1000, ms / 1000);

test('an image unused for longer than the retention period is removed with its records; recent and returned ones stay', async () => {
  const folder = tempDir('graph-pin-prune-');
  const source = join(folder, 'octocode');
  const cache = join(folder, 'cache');
  writeFileSync(source, 'first');
  const first = await freezeExecutable(source, cache, { now: T0, alive: nobody });
  writeFileSync(source, 'second');
  const second = await freezeExecutable(source, cache, { now: T0 + HOUR, alive: nobody });
  assert.ok(existsSync(first), 'an image used within the retention period stays');
  assert.equal(digestsNaming(cache, first).length, 1);

  // The one that stays recent is the one returned now; the first went unused past the retention period.
  const later = T0 + IMAGE_RETENTION_MS + 2 * HOUR;
  assert.equal(await freezeExecutable(source, cache, { now: later, alive: nobody }), second);
  assert.ok(!existsSync(first), 'an image unused past the retention period is removed');
  assert.ok(!existsSync(join(cache, 'used', basename(first))));
  assert.deepEqual(digestsNaming(cache, first), []);
  assert.ok(existsSync(second));
  assert.equal(readFileSync(second, 'utf8'), 'second');

  // The image being returned is kept however old its last use is.
  assert.equal(await freezeExecutable(source, cache, { now: later + 10 * IMAGE_RETENTION_MS, alive: nobody }), second);
  assert.ok(existsSync(second));
});

test('an image a live process pinned is never removed, whatever its age', async () => {
  const folder = tempDir('graph-pin-live-');
  const source = join(folder, 'octocode');
  const cache = join(folder, 'cache');
  writeFileSync(source, 'held');
  const held = await freezeExecutable(source, cache, { now: T0, alive: nobody });
  writeFileSync(source, 'current');
  const late = T0 + 10 * IMAGE_RETENTION_MS;
  // This process recorded its use of `held`: while it lives, the image stays.
  await freezeExecutable(source, cache, { now: late, alive: pid => pid === process.pid });
  assert.ok(existsSync(held), 'a live process still uses the image');
  await freezeExecutable(source, cache, { now: late, alive: nobody });
  assert.ok(!existsSync(held), 'once no process holds it, age decides');
});

test('stale staging copies and images without use records go by age; a live copy stays', async () => {
  const folder = tempDir('graph-pin-staging-');
  const source = join(folder, 'octocode');
  const cache = join(folder, 'cache');
  writeFileSync(source, 'current');
  const current = await freezeExecutable(source, cache, { now: T0, alive: nobody });
  const now = T0 + 2 * IMAGE_RETENTION_MS;
  const old = now - IMAGE_RETENTION_MS - HOUR;
  const recent = now - HOUR;
  const entry = name => join(cache, name);
  // Written by a cache that predates use records: an image's own time is its last use.
  writeFileSync(entry('a'.repeat(64)), 'old image');
  setTime(entry('a'.repeat(64)), old);
  writeFileSync(entry('b'.repeat(64)), 'recent image');
  setTime(entry('b'.repeat(64)), recent);
  for (const [name, time] of [['copy-AbC123', old], ['copy-XyZ789', recent], ['copy-4242-Qw3rTy', old], ['copy-4343-Qw3rTy', old]]) {
    mkdirSync(entry(name));
    setTime(entry(name), time);
  }
  assert.equal(await freezeExecutable(source, cache, { now, alive: pid => pid === 4242 }), current);
  const left = readdirSync(cache).filter(name => name !== 'digests' && name !== 'used').toSorted();
  assert.deepEqual(left, [basename(current), 'b'.repeat(64), 'copy-4242-Qw3rTy', 'copy-XyZ789'].toSorted());
});
