import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releaseTargets, verifyReleaseEvidence } from '../src/verify-release-evidence.mjs';
import { tempWorkspace } from './helpers.mjs';
const revision = 'a'.repeat(40), version = '0.1.0';
function fixture(t) {
  const root = tempWorkspace(t, 'communication-release-receipts-');
  for (const target of releaseTargets) {
    const directory = join(root, target); mkdirSync(directory);
    const archive = `skill-${target}.tar.gz`, bytes = Buffer.from(target);
    writeFileSync(join(directory, archive), bytes);
    const result = {
      target, version, sourceRevision: revision, passed: true, dirty: false,
      binarySha256: 'b'.repeat(64), harnessSha256: 'c'.repeat(64), skillSha256: 'd'.repeat(64),
      launcher: { passed: true }, restore: true,
      package: { archive: target.includes('windows') ? `D:\\build\\${archive}` : `/build/${archive}`, targets: [target], sha256: createHash('sha256').update(bytes).digest('hex'), verification: { [target]: { startup: { passed: true }, skill: { passed: true } } } },
    };
    writeFileSync(join(directory, 'release-smoke.json'), JSON.stringify(result));
  }
  return root;
}
test('native gate accepts complete same-revision receipts and resolves Windows archive paths on Unix', t => {
  assert.equal(verifyReleaseEvidence(fixture(t), revision, version).targets.length, 6);
});
for (const failure of ['missing', 'dirty', 'revision', 'unexecuted', 'checksum', 'restore', 'mixed-harness']) test(`native gate rejects ${failure} evidence`, t => {
  const root = fixture(t), target = releaseTargets[0], directory = join(root, target), path = join(directory, 'release-smoke.json');
  const result = JSON.parse(readFileSync(path, 'utf8'));
  if (failure === 'missing') rmSync(directory, { recursive: true });
  else {
    if (failure === 'dirty') result.dirty = true;
    if (failure === 'revision') result.sourceRevision = 'e'.repeat(40);
    if (failure === 'unexecuted') result.package.verification[target].startup.passed = null;
    if (failure === 'checksum') writeFileSync(join(directory, `skill-${target}.tar.gz`), 'tampered');
    if (failure === 'restore') result.restore = false;
    if (failure === 'mixed-harness') result.harnessSha256 = 'f'.repeat(64);
    writeFileSync(path, JSON.stringify(result));
  }
  assert.throws(() => verifyReleaseEvidence(root, revision, version));
});

test('the native matrix covers every supported build target', () => {
  const root = fileURLToPath(new URL('../', import.meta.url));
  const workflow = readFileSync(join(root, '../../.github/workflows/agents-communication.yml'), 'utf8');
  const matrix = [...workflow.matchAll(/^\s+target: ([a-z0-9_-]+)$/gm)].map(match => match[1]);
  assert.deepEqual(matrix.sort(), [...releaseTargets].sort());
  assert.equal(workflow.split("'packages/octocode-config/rust/**'").length - 1, 2, 'Push and PR filters must cover the shared native home module');
});

for (const digest of [undefined, 'not-a-sha256']) test(`native gate rejects every receipt having ${digest === undefined ? 'no' : 'an invalid'} skill digest`, t => {
  const root = fixture(t);
  for (const target of releaseTargets) {
    const path = join(root, target, 'release-smoke.json');
    const result = JSON.parse(readFileSync(path, 'utf8'));
    if (digest === undefined) delete result.skillSha256;
    else result.skillSha256 = digest;
    writeFileSync(path, JSON.stringify(result));
  }
  assert.throws(() => verifyReleaseEvidence(root, revision, version), /Missing or invalid skill digest/);
});
