import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { staleArtifacts, stagedNativeArtifacts } from './artifact-freshness.mjs';

const FINGERPRINT = 'aa8c48f2b9d5159056b3e8d61b4d2d6c9b50dd37e725b5f4d524ee33169c5a06';
const VERSION = '20.2.0';

function stage(root, platform, name, body) {
  const dir = path.join(root, platform);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, name), body);
  return path.join(dir, name);
}

test('an artifact that lacks the current contract fingerprint is stale', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-fresh-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const fresh = stage(root, 'darwin-arm64', 'octocode', `x"fingerprint":"${FINGERPRINT}" ${VERSION}`);
  const old = stage(root, 'darwin-x64', 'octocode', `x"fingerprint":"deadbeef" ${VERSION}`);
  assert.deepEqual(staleArtifacts([fresh, old], FINGERPRINT, VERSION), [old]);
});

test('an artifact from another version is stale even with the current fingerprint', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-fresh-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const wrong = stage(root, 'linux-x64-gnu', 'octocode', `${FINGERPRINT} 20.0.0`);
  assert.deepEqual(staleArtifacts([wrong], FINGERPRINT, VERSION), [wrong]);
});

test('staged native artifacts are the binary of each platform, and nothing else', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-fresh-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const binary = stage(root, 'darwin-arm64', 'octocode', 'x');
  stage(root, 'darwin-arm64', 'octocode-native.darwin-arm64.node', 'x');
  stage(root, 'darwin-arm64', 'package.json', '{}');
  stage(root, 'win32-x64-msvc', 'octocode.exe', 'x');
  assert.deepEqual(stagedNativeArtifacts(root).sort(), [binary, path.join(root, 'win32-x64-msvc', 'octocode.exe')].sort());
});
