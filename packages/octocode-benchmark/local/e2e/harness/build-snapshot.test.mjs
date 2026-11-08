// The run's build snapshot is immune to a rebuild rewriting the sources in place.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('snapshot keeps running the copies after the native binary is overwritten, then removes itself', () => {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-snapshot-test-'));
  try {
    const source = path.join(scratch, 'octocode');
    fs.writeFileSync(source, 'original binary bytes');
    const script = `
      import fs from 'node:fs';
      import { snapshotBuild } from ${JSON.stringify(path.resolve(import.meta.dirname, 'build-snapshot.mjs'))};
      import { nativeBinaryStamp } from ${JSON.stringify(path.resolve(import.meta.dirname, 'native-binary.mjs'))};
      const manifest = snapshotBuild();
      fs.writeFileSync(${JSON.stringify(source)}, '');
      const stamp = nativeBinaryStamp();
      console.log(JSON.stringify({ dir: manifest.dir, copy: manifest.native.copy, copyText: fs.readFileSync(manifest.native.copy, 'utf8'), env: process.env.OCTOCODE_NATIVE_BIN, stamp, cli: fs.existsSync(manifest.cli.copy + '/octocode.js'), mcp: fs.existsSync(manifest.mcp.copy + '/index.js') }));
    `;
    const run = spawnSync(process.execPath, ['--input-type=module', '-e', script], { encoding: 'utf8', env: { ...process.env, NODE_ENV: 'test', OCTOCODE_NATIVE_BIN: source, OCTOCODE_E2E_SNAPSHOT: '' } });
    assert.equal(run.status, 0, run.stderr);
    const seen = JSON.parse(run.stdout);
    assert.equal(seen.copyText, 'original binary bytes');
    assert.equal(seen.env, seen.copy);
    assert.ok(seen.cli && seen.mcp);
    assert.equal(seen.stamp.snapshot.native.changedSinceSnapshot, true);
    assert.equal(seen.stamp.sha256, seen.stamp.snapshot.native.sha256);
    assert.equal(fs.existsSync(seen.dir), false, 'the owner removes the snapshot at exit');
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
});
