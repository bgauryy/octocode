import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

const script = new URL('./interview-grade.mjs', import.meta.url).pathname;
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });

test('grader rejects missing call files and a runner without both placeholders', () => {
  const dir = mkdtempSync(join(tmpdir(), 'interview-grade-'));
  try {
    const command = `${process.execPath} -e 'process.exit(0)' {tool} {input}`;
    assert.notEqual(run(dir, '--cmd', command).status, 0);
    writeFileSync(join(dir, 'G1-calls.jsonl'), '{"scenario":"S1","step":1,"tool":"localFetch","input":{"queries":[]}}\n');
    assert.notEqual(run(dir, '--cmd', `${process.execPath} -e 'process.exit(0)'`).status, 0);
    assert.notEqual(run(dir, '--cmd', command, '--timeout', '0').status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('grader reports valid calls and fails malformed or non-gradable input', () => {
  const dir = mkdtempSync(join(tmpdir(), 'interview-grade-'));
  try {
    const file = join(dir, 'G1-calls.jsonl');
    const command = `${process.execPath} -e 'process.exit(0)' {tool} {input}`;
    writeFileSync(file, '{"scenario":"S1","step":1,"tool":"localFetch","input":{"path":"a.ts"},"outcome":"badLine"}\n');
    const valid = run(dir, '--cmd', command, '--json');
    assert.equal(valid.status, 0);
    assert.deepEqual(JSON.parse(valid.stdout).total, { gradable: 1, valid: 1, validRate: 1, rejected: 0 });
    writeFileSync(file, 'not json\n');
    assert.equal(run(dir, '--cmd', command).status, 2);
    writeFileSync(file, '{"scenario":"S1","step":1,"tool":"localFetch","input":"not an object"}\n');
    assert.equal(run(dir, '--cmd', command).status, 2);
    writeFileSync(file, '{"scenario":"S1","step":1,"tool":"localFetch","input":{"path":"<path>"}}\n');
    assert.equal(run(dir, '--cmd', command).status, 2);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('runner receives quoted arguments and literal placeholders without shell effects', () => {
  const dir = mkdtempSync(join(tmpdir(), 'interview-grade-'));
  try {
    const received = join(dir, 'received.json');
    const marker = join(dir, 'unexpected');
    const runner = join(dir, 'runner.mjs');
    writeFileSync(runner, `import { writeFileSync } from 'node:fs';\nwriteFileSync(${JSON.stringify(received)}, JSON.stringify(process.argv.slice(2)));\n`);
    const tool = `local'{input}$(touch ${marker})`;
    const input = { queries: [{ path: "a'b{tool}.ts" }] };
    writeFileSync(join(dir, 'G1-calls.jsonl'), `${JSON.stringify({ scenario: 'S1', step: 1, tool, input })}\n`);
    const result = run(dir, '--cmd', `${process.execPath} ${runner} {tool} {input}`);
    assert.equal(result.status, 0);
    assert.deepEqual(JSON.parse(readFileSync(received, 'utf8')), [tool, JSON.stringify(input)]);
    assert.equal(existsSync(marker), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
