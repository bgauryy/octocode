import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { checkStartup, digest, payloadDigest, payloadFiles } from '../src/artifact-checks.mjs';
import { packSkill } from '../src/pack-skill.mjs';
import { root, tempWorkspace } from './helpers.mjs';

function fixture(t) {
  const directory = tempWorkspace(t, 'communication-artifact-');
  for (const entry of ['package.json', 'SKILL.md', 'scripts'])
    cpSync(join(root, entry), join(directory, entry), { recursive: true });
  return { directory, entry: join(directory, 'scripts/communication.py') };
}

test('portable startup verifies Python identity and rejects malformed help', t => {
  const f = fixture(t);
  assert.equal(checkStartup(f.entry).passed, true);
  writeFileSync(f.entry, 'print("{}")');
  assert.throws(() => checkStartup(f.entry), /Unexpected --help/);
  writeFileSync(f.entry, 'print("invalid")');
  assert.throws(() => checkStartup(f.entry), /verification failed/);
});

test('startup timeout is bounded without retries', t => {
  const f = fixture(t);
  writeFileSync(f.entry, 'import time; time.sleep(60)');
  const started = performance.now();
  assert.throws(() => checkStartup(f.entry, 100), /ETIMEDOUT/);
  assert.ok(performance.now() - started < 1500);
});

test('pack extracts and verifies Python, skill, schema, database and launcher', t => {
  const f = fixture(t), packed = packSkill(f.directory);
  assert.ok(existsSync(packed.archive));
  assert.equal(packed.format, 'portable-python');
  assert.equal(packed.sha256, digest(packed.archive));
  assert.equal(packed.runtimeSha256, payloadDigest(join(f.directory, 'scripts')));
  for (const gate of ['startup', 'skill', 'schema', 'database'])
    assert.equal(packed.verification[gate].passed, true, gate);
  assert.equal(packed.launcher.passed, true);
  assert.equal(readdirSync(join(f.directory, 'out')).some(name => name.startsWith('.communication-pack-')), false);
});

test('failed extracted package verification preserves prior archive and cleans staging', t => {
  const f = fixture(t), packed = packSkill(f.directory), before = readFileSync(packed.archive);
  writeFileSync(f.entry, 'print("{}")');
  assert.throws(() => packSkill(f.directory), /Unexpected --help/);
  assert.deepEqual(readFileSync(packed.archive), before);
  assert.equal(readdirSync(join(f.directory, 'out')).some(name => name.startsWith('.communication-pack-')), false);
});

test('runtime payload excludes caches and detects module changes', t => {
  const f = fixture(t), scripts = join(f.directory, 'scripts'), before = payloadDigest(scripts);
  mkdirSync(join(scripts, '__pycache__'), { recursive: true });
  writeFileSync(join(scripts, '__pycache__/cached.pyc'), 'cache');
  assert.equal(payloadDigest(scripts), before);
  assert.ok(!payloadFiles(scripts).some(path => path.endsWith('.pyc')));
  writeFileSync(join(scripts, 'communication/cli.py'), '# changed module');
  assert.notEqual(payloadDigest(scripts), before);
});
