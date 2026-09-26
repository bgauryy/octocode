import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

export const releaseTargets = [
  'aarch64-apple-darwin', 'x86_64-apple-darwin',
  'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu',
  'aarch64-pc-windows-msvc', 'x86_64-pc-windows-msvc',
];
function receipts(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? receipts(path) : entry.isFile() && entry.name === 'release-smoke.json' ? [path] : [];
  });
}
export function verifyReleaseEvidence(directory, revision, version) {
  assert.match(revision ?? '', /^[a-f0-9]{40}$/, 'Exact tested Git revision required');
  const targets = new Map();
  let harness, skill;
  for (const path of receipts(directory)) {
    const result = JSON.parse(readFileSync(path, 'utf8'));
    assert.ok(releaseTargets.includes(result.target), `Unsupported release target: ${result.target}`);
    assert.ok(!targets.has(result.target), `Duplicate target receipt: ${result.target}`);
    assert.equal(result.passed, true, `${result.target}: smoke failed`);
    assert.equal(result.dirty, false, `${result.target}: uncommitted build source`);
    assert.equal(result.sourceRevision, revision, `${result.target}: wrong revision`);
    assert.equal(result.version, version, `${result.target}: wrong package version`);
    assert.match(result.binarySha256 ?? '', /^[a-f0-9]{64}$/, 'Missing native binary digest');
    assert.match(result.harnessSha256 ?? '', /^[a-f0-9]{64}$/, 'Missing harness digest');
    assert.match(result.skillSha256 ?? '', /^[a-f0-9]{64}$/, 'Missing or invalid skill digest');
    assert.equal(result.harnessSha256, harness ??= result.harnessSha256, 'All targets must execute the same harness');
    assert.equal(result.skillSha256, skill ??= result.skillSha256, 'All targets must ship the same skill');
    assert.deepEqual(result.package.targets, [result.target], 'Release archives must contain only their tested native target');
    assert.equal(result.launcher?.passed, true, `${result.target}: launcher was not executed`);
    const checks = result.package?.verification?.[result.target];
    assert.equal(checks?.startup?.passed, true, `${result.target}: native startup was not executed`);
    assert.equal(checks?.skill?.passed, true, `${result.target}: embedded skill was not checked`);
    assert.equal(result.restore, true, `${result.target}: missing restore check`);
    const archive = join(dirname(path), basename(result.package.archive.replaceAll('\\', '/')));
    const actual = createHash('sha256').update(readFileSync(archive)).digest('hex');
    assert.equal(actual, result.package.sha256, `${result.target}: archive checksum changed`);
    targets.set(result.target, { target: result.target, archive: basename(archive), sha256: actual, binarySha256: result.binarySha256 });
  }
  assert.deepEqual([...targets.keys()].sort(), [...releaseTargets].sort(), 'Every launcher target requires its own native execution receipt');
  return { passed: true, scope: 'native-core-artifact-gate', revision, version, targets: [...targets.values()], vendorCompatibility: 'separate authenticated live evaluation required', publisherIdentity: 'not certified by this gate' };
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
  console.log(JSON.stringify(verifyReleaseEvidence(resolve(process.argv[2] ?? 'release-artifacts'), process.argv[3], version)));
}
