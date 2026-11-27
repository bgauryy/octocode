import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { HARNESS } from './lib.mjs';
import { scratch } from './testing.mjs';

/** Runs the wrapper against a stand-in gh that echoes its arguments. */
function wrapper(dir, args) {
  const real = path.join(dir, 'real-gh.mjs');
  fs.writeFileSync(real, '#!/usr/bin/env node\nprocess.stdout.write(`ran ${JSON.stringify(process.argv.slice(2))} é😀`);\nprocess.exit(3);\n');
  fs.chmodSync(real, 0o755);
  const log = path.join(dir, 'gh-log.jsonl');
  const ran = spawnSync(path.join(HARNESS, 'gh-readonly'), args, { encoding: 'utf8', env: { ...process.env, BENCH_GH_REAL: real, BENCH_GH_LOG: log, RUN_ID: 'p1-s-Q1-gh' } });
  const rows = fs.existsSync(log) ? fs.readFileSync(log, 'utf8').trim().split('\n').map(line => JSON.parse(line)) : [];
  return { ...ran, row: rows.at(-1), rows };
}

test('a read passes through unchanged with its exit code, and is logged in code points', () => {
  const { dir, cleanup } = scratch('gh-read');
  try {
    const args = ['api', 'repos/o/r/contents/a.md', '--jq', '.content'];
    const ran = wrapper(dir, args);
    assert.equal(ran.stdout, `ran ${JSON.stringify(args)} é😀`);
    assert.equal(ran.status, 3);
    assert.deepEqual(ran.row.args, args);
    assert.equal(ran.row.refused, null);
    assert.equal(ran.row.exit, ran.status);
    assert.equal(ran.row.stdoutChars, [...ran.stdout].length);
    assert.equal(ran.row.run, 'p1-s-Q1-gh');
  } finally {
    cleanup();
  }
});

test('writes are refused before the real gh runs, and the refusal is logged', () => {
  const { dir, cleanup } = scratch('gh-write');
  try {
    const writes = [['pr', 'create'], ['issue', 'comment', '1', '-b', 'x'], ['api', '-X', 'POST', 'repos/o/r/issues'], ['api', '--method=DELETE', 'repos/o/r'], ['api', 'repos/o/r/issues', '-f', 'title=x'], ['api', 'graphql', '-f', 'query=mutation { x }'], ['repo', 'delete', 'o/r'], []];
    for (const args of writes) {
      const ran = wrapper(dir, args);
      assert.equal(ran.status, 2, args.join(' '));
      assert.equal(ran.stdout, '', args.join(' '));
      assert.match(ran.stderr, /read-only benchmark\): refused/);
      assert.equal(typeof ran.row.refused, 'string');
    }
    const reads = [['api', '-X', 'GET', 'search/code', '-f', 'q=x'], ['api', 'graphql', '-f', 'query={ viewer { login } }'], ['pr', 'view', '1', '-R', 'o/r'], ['search', 'code', 'x'], ['--version']];
    for (const args of reads) assert.equal(wrapper(dir, args).row.refused, null, args.join(' '));
  } finally {
    cleanup();
  }
});
