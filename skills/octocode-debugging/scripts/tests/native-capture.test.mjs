import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync, mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
const script = resolve('skills/octocode-debugging/scripts/native-capture.sh');
test('Linux backend passes PID, finite window and paths without shell expansion', () => {
  const dir = mkdtempSync(join(tmpdir(), 'native-capture-test-'));
  try {
    const bin = join(dir, 'bin'); mkdirSync(bin);
    for (const [name, body] of Object.entries({
      uname: 'echo Linux', ps: 'echo "123 1 1024 2.0 00:01 node"',
      perf: 'printf "%s\\n" "$@" >> "$CALLS"; echo "sampled stack"',
    })) writeFileSync(join(bin, name), `#!/bin/sh\n${body}\n`, { mode: 0o700 });
    const out = join(dir, 'capture with spaces');
    const calls = join(dir, 'calls');
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, CALLS: calls };
    const result = spawnSync('bash', [script, '123', '2', out], { env, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const args = readFileSync(calls, 'utf8');
    assert.match(args, /-p\n123\n/);
    assert.match(args, /--\nsleep\n2\n/);
    assert.match(readFileSync(join(out, 'stacks.txt'), 'utf8'), /sampled stack/);
    assert.notEqual(spawnSync('bash', [script, '123', '2', out], { env }).status, 0);
    assert.equal(spawnSync('bash', [script, '123;exit', '2', join(dir, 'bad')], { env }).status, 2);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
test('metadata permission failure preserves diagnostic and stops capture', () => {
  const dir = mkdtempSync(join(tmpdir(), 'native-capture-denied-'));
  try {
    const bin = join(dir, 'bin'); mkdirSync(bin);
    for (const [name, body] of Object.entries({ uname: 'echo Linux', perf: 'exit 99', ps: 'echo "permission denied" >&2; exit 1' })) {
      writeFileSync(join(bin, name), `#!/bin/sh\n${body}\n`, { mode: 0o700 });
    }
    const out = join(dir, 'capture');
    const result = spawnSync('bash', [script, '123', '1', out], { env: { ...process.env, PATH: `${bin}:${process.env.PATH}` }, encoding: 'utf8' });
    assert.equal(result.status, 1);
    assert.match(readFileSync(join(out, 'profiler.log'), 'utf8'), /permission denied/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
