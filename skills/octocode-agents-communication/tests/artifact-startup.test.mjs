import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { checkStartup, digest, installExecutable, verifyExecutable, verifyStartup } from '../src/artifact-checks.mjs';
import { packSkill } from '../src/pack-skill.mjs';
import { binary, tempWorkspace } from './helpers.mjs';

const options = { target: 'fixture', hostTarget: 'fixture', timeoutMs: 10000 };
// Successful install/pack checks exercise the shipped artifact, not shell help imitations.
const skill = readFileSync(new URL('../SKILL.md', import.meta.url), 'utf8');
const invalid = '#!/bin/sh\nprintf "%s\\n" "{}"\n';
function fixture(t) {
  const directory = tempWorkspace(t, 'communication-artifact-');
  const executable = (name, content) => {
    const path = join(directory, name);
    if (content === undefined) copyFileSync(binary, path); else writeFileSync(path, content);
    chmodSync(path, 0o755); return path;
  };
  return { directory, executable };
}

test('identical rebuilds preserve installed inode and verify the existing executable', { skip: process.platform === 'win32' }, t => {
  const f = fixture(t), source = f.executable('source'), destination = join(f.directory, 'installed');
  assert.equal(installExecutable(source, destination, options).changed, true);
  const inode = statSync(destination).ino;
  assert.equal(installExecutable(source, destination, options).changed, false);
  assert.equal(statSync(destination).ino, inode);
});

test('failed candidate startup preserves last executable and removes temporary files', { skip: process.platform === 'win32' }, t => {
  const f = fixture(t), destination = f.executable('installed'), source = f.executable('bad', invalid);
  const before = digest(destination);
  assert.throws(() => installExecutable(source, destination, options), /Unexpected --help/);
  assert.equal(digest(destination), before);
  assert.equal(readdirSync(f.directory).some(name => name.endsWith('.tmp')), false);
});

test('startup timeout is bounded, does not retry and rejects malformed help', { skip: process.platform === 'win32' }, t => {
  const f = fixture(t), stalled = f.executable('stalled', '#!/bin/sh\nexec sleep 60\n');
  const started = performance.now();
  assert.throws(() => checkStartup(stalled, 100), /ETIMEDOUT/);
  assert.ok(performance.now() - started < 1500);
  assert.throws(() => checkStartup(f.executable('malformed', '#!/bin/sh\necho invalid\n')), /verification failed/);
  assert.equal(verifyExecutable(stalled, { target: 'foreign', hostTarget: 'fixture' }).startup.passed, null);
});

function packageFixture(t) {
  const f = fixture(t), directory = join(f.directory, '.');
  const bin = join(directory, 'scripts'); mkdirSync(bin, { recursive: true });
  writeFileSync(join(f.directory, 'package.json'), '{"version":"0.1.0"}');
  writeFileSync(join(directory, 'SKILL.md'), skill);
  const executable = join(bin, 'octocode-agents-communication');
  const update = content => {
    if (content === undefined) copyFileSync(binary, executable); else writeFileSync(executable, content);
    chmodSync(executable, 0o755);
    writeFileSync(join(bin, 'SHA256SUMS'), `${digest(executable)}  octocode-agents-communication\n`);
  };
  update();
  const launcher = join(directory, 'scripts/agents-communication');
  writeFileSync(launcher, '#!/bin/sh\nexec "$(dirname "$0")/octocode-agents-communication" "$@"\n'); chmodSync(launcher, 0o755);
  return { ...f, skillDirectory: directory, bin, executable, update };
}

test('pack extracts and verifies native binary, embedded skill and launcher before publishing', { skip: process.platform === 'win32' }, t => {
  const f = packageFixture(t), packed = packSkill(f.directory, options);
  assert.ok(existsSync(packed.archive)); assert.equal(packed.sha256, digest(packed.archive));
  assert.equal(packed.verification.fixture.startup.passed, true);
  assert.equal(packed.verification.fixture.skill.passed, true); assert.equal(packed.launcher.passed, true);
  assert.equal(readdirSync(join(f.directory, 'out')).some(name => name.startsWith('.communication-pack-')), false);
});

test('failed package verification preserves prior archive and leaves no staging artifacts', { skip: process.platform === 'win32' }, t => {
  const f = packageFixture(t), packed = packSkill(f.directory, options), before = readFileSync(packed.archive);
  f.update(invalid);
  assert.throws(() => packSkill(f.directory, options), /Unexpected --help/);
  assert.deepEqual(readFileSync(packed.archive), before);
  f.update(); writeFileSync(join(f.directory, 'SKILL.md'), 'changed');
  assert.throws(() => packSkill(f.directory, options), /Embedded skill differs/);
  assert.deepEqual(readFileSync(packed.archive), before);
  writeFileSync(join(f.bin, 'incomplete.tmp'), 'unfinished build');
  assert.throws(() => packSkill(f.directory, options), /Unexpected runtime artifact/);
  rmSync(join(f.bin, 'incomplete.tmp'));
  writeFileSync(join(f.bin, 'SHA256SUMS'), 'wrong checksum');
  assert.throws(() => packSkill(f.directory, options), /Checksum mismatch/);
  assert.equal(readdirSync(join(f.directory, 'out')).some(name => name.startsWith('.communication-pack-')), false);
});

// A foreign executable may be built, but cannot pass native archive validation here.
test('pack refuses foreign targets before creating an archive', t => {
  const f = fixture(t);
  assert.throws(() => packSkill(f.directory, {hostTarget: 'native', target: 'foreign'}), /native validation on the target platform/);
  assert.equal(existsSync(join(f.directory, 'out')), false);
});


test('cold assessment and normal startup remain separate bounded gates', { skip: process.platform !== 'darwin' }, t => {
  const f=fixture(t), executable=f.executable('cold-native');
  const result=verifyStartup(executable,{coldStart:true});
  assert.equal(result.coldStart.passed,true);assert.equal(result.coldStart.timeoutMs,60000);
  assert.equal(result.passed,true);assert.equal(result.timeoutMs,10000);
  const stalled=f.executable('cold-stalled','#!/bin/sh\nexec sleep 60\n');
  assert.throws(()=>verifyStartup(stalled,{coldStart:true,coldTimeoutMs:100,timeoutMs:100}),/ETIMEDOUT/);
});
