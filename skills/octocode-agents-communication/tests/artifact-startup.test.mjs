import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { checkStartup, digest, installExecutable, verifyExecutable } from '../src/artifact-checks.mjs';
import { packSkill } from '../src/pack-skill.mjs';
import { tempWorkspace } from './helpers.mjs';

const options = { target: 'fixture', hostTarget: 'fixture', timeoutMs: 10000 };
const skill = '---\nname: fixture\n---\nUse the CLI.\n';
const valid = `#!/bin/sh\nif [ "$1" = skill ]; then\n  printf '%s\\n' '${JSON.stringify({ instructions: skill })}'\nelse\n  printf '%s\\n' '{"package":"@octocodeai/octocode-agents-communication","implementation":"Rust"}'\nfi\n`;
const invalid = '#!/bin/sh\nprintf "%s\\n" "{}"\n';
function fixture(t) {
  const directory = tempWorkspace(t, 'communication-artifact-');
  const executable = (name, content = valid) => {
    const path = join(directory, name); writeFileSync(path, content); chmodSync(path, 0o755); return path;
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
    writeFileSync(executable, content); chmodSync(executable, 0o755);
    writeFileSync(join(bin, 'SHA256SUMS'), `${digest(executable)}  octocode-agents-communication\n`);
  };
  update(valid);
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
  f.update(valid); writeFileSync(join(f.directory, 'SKILL.md'), 'changed');
  assert.throws(() => packSkill(f.directory, options), /Embedded skill differs/);
  assert.deepEqual(readFileSync(packed.archive), before);
  writeFileSync(join(f.bin, 'incomplete.tmp'), 'unfinished build');
  assert.throws(() => packSkill(f.directory, options), /Unexpected runtime artifact/);
  rmSync(join(f.bin, 'incomplete.tmp'));
  writeFileSync(join(f.bin, 'SHA256SUMS'), 'wrong checksum');
  assert.throws(() => packSkill(f.directory, options), /Checksum mismatch/);
  assert.equal(readdirSync(join(f.directory, 'out')).some(name => name.startsWith('.communication-pack-')), false);
});
