import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { runPath } from './lib.mjs';
import { plan, preflight, runAll, session } from './run.mjs';
import { roundIn, scratch } from './testing.mjs';

const twoBuilds = dir => ({
  pins: { base: { path: path.join(dir, 'base-bin'), sha256: 'a'.repeat(64) }, cand: { path: path.join(dir, 'cand-bin'), sha256: 'b'.repeat(64) } },
});
const buildArm = pin => ({ kind: 'mcp', pin, instruction: 'Use the tools.', server: { command: 'node', args: ['{repo}/server.js'], env: { NODE_ENV: 'development', OCTOCODE_NATIVE_BIN: '{pin}', OCTOCODE_HOME: '{home}' } } });

test('passes are interleaved: every run of pass n starts before any run of pass n+1, arms of a question side by side', () => {
  const { dir, cleanup } = scratch('plan');
  try {
    const manifest = roundIn(dir, { passes: 3, arms: { 'v20-base': buildArm('base'), 'v20-cand': buildArm('cand'), gh: { kind: 'shell', instruction: 'Use gh.' } }, extra: twoBuilds(dir) });
    const tasks = plan(manifest);
    const arms = Object.keys(manifest.arms);
    assert.equal(tasks.length, manifest.passes * manifest.sets.s.ids.length * arms.length);
    assert.deepEqual(tasks.map(task => task.pass), tasks.map(task => task.pass).toSorted((a, b) => a - b));
    for (let at = 0; at < tasks.length; at += arms.length) {
      const group = tasks.slice(at, at + arms.length);
      assert.equal(new Set(group.map(task => `${task.pass}-${task.id}`)).size, 1);
      assert.deepEqual(group.map(task => task.arm).toSorted(), arms.toSorted());
    }
    // No arm always goes first.
    assert.ok(new Set(tasks.filter((_, at) => at % arms.length === 0).map(task => task.arm)).size > 1);
    assert.ok(plan(manifest, { pass: 2, arms: new Set(['gh']) }).every(task => task.pass === 2 && task.arm === 'gh'));
  } finally {
    cleanup();
  }
});

test('two Octocode arms each start the server with their own pinned binary and home', () => {
  const { dir, cleanup } = scratch('session');
  try {
    const manifest = roundIn(dir, { arms: { 'v20-base': buildArm('base'), 'v20-cand': buildArm('cand'), gh: { kind: 'shell', instruction: 'Use gh.', filters: ['jq'] } }, extra: twoBuilds(dir) });
    const [base, cand, gh] = ['v20-base', 'v20-cand', 'gh'].map(arm => session(manifest, plan(manifest).find(task => task.arm === arm), { scratch: path.join(dir, 'scratch', arm), token: 't', ghReal: '/real/gh', ghBin: path.join(dir, 'bin') }));
    const server = made => made.config.mcpServers.octocode;
    assert.equal(server(base).env.OCTOCODE_NATIVE_BIN, manifest.pins.base.path);
    assert.equal(server(cand).env.OCTOCODE_NATIVE_BIN, manifest.pins.cand.path);
    assert.notEqual(server(base).env.OCTOCODE_HOME, server(cand).env.OCTOCODE_HOME);
    assert.equal(gh.config, undefined);
    assert.equal(gh.env.PATH.split(path.delimiter)[0], path.join(dir, 'bin'));
    assert.ok(gh.args.includes('Bash(gh:*)') && gh.args.includes('Bash(jq:*)'));
    assert.match(base.prompt, /What is Q\d\?\n\n\nUse the tools\.$/);
    for (const made of [base, cand, gh]) assert.ok(!Object.keys(made.env).some(key => key.startsWith('OCTOCODE')));
  } finally {
    cleanup();
  }
});

test('preflight refuses a neutral directory that sits under project instructions', () => {
  const { dir, cleanup } = scratch('preflight');
  try {
    const manifest = roundIn(dir, { arms: { gh: { kind: 'shell', instruction: 'Use gh.' } } });
    assert.throws(() => preflight(manifest, plan(manifest), process.execPath), /project instructions/);
  } finally {
    cleanup();
  }
});

test('a run stores its output, removes its scratch (read-only caches too) and is never overwritten', async () => {
  const { dir, cleanup } = scratch('run');
  try {
    // A stand-in host: writes a read-only module cache into the session home, then prints a result event.
    const host = path.join(dir, 'host.mjs');
    fs.writeFileSync(host, `#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
const args = process.argv.slice(2);
const config = args.includes('--mcp-config') ? JSON.parse(fs.readFileSync(args[args.indexOf('--mcp-config') + 1], 'utf8')) : undefined;
const home = config?.mcpServers.octocode.env.OCTOCODE_HOME;
if (home) {
  const cache = path.join(home, 'go', 'pkg', 'mod', 'm@v1');
  fs.mkdirSync(cache, { recursive: true });
  fs.writeFileSync(path.join(cache, 'f.go'), 'package m');
  fs.chmodSync(cache, 0o555);
}
console.log(JSON.stringify({ type: 'system', subtype: 'init', tools: [] }));
console.log(JSON.stringify({ type: 'result', result: 'answer from ' + process.env.RUN_ID + ' in ' + (home ?? 'no home'), num_turns: 1, total_cost_usd: 0.01, usage: {} }));
`);
    fs.chmodSync(host, 0o755);
    const manifest = roundIn(dir, { passes: 2, ids: ['Q1', 'Q2'] });
    fs.mkdirSync(manifest.neutralCwd, { recursive: true });
    const tasks = plan(manifest);
    const lines = [];
    const context = { host, token: 't', ghReal: '/real/gh', jobs: 3, log: line => lines.push(line) };
    assert.equal(await runAll(manifest, tasks, context), 0);
    assert.equal(lines.length, tasks.length);
    // The test round's scratch folder is the test folder itself: only what the runs put there must be gone.
    for (const made of ['pass-1', 'pass-2', 'bin']) assert.equal(fs.existsSync(path.join(manifest.scratch, made)), false, `${made} is removed after the runs`);
    for (const task of tasks) {
      const out = runPath(manifest, task.pass, task.set, task.id, task.arm);
      assert.ok(out.includes(`${path.sep}pass-${task.pass}${path.sep}runs${path.sep}`));
      const stored = JSON.parse(fs.readFileSync(path.join(out, 'result.json'), 'utf8'));
      assert.equal(stored.rc, 0);
      assert.equal(stored.pass, task.pass);
      assert.match(fs.readFileSync(path.join(out, 'answer.md'), 'utf8'), new RegExp(`answer from p${task.pass}-${task.name} `));
      if (manifest.arms[task.arm].kind === 'mcp') assert.ok(stored.home.startsWith(manifest.scratch) && !fs.existsSync(stored.home));
    }
    const kept = runPath(manifest, 1, 's', 'Q1', 'tool');
    fs.writeFileSync(path.join(kept, 'answer.md'), 'stored answer');
    lines.length = 0;
    assert.equal(await runAll(manifest, tasks, context), 0);
    assert.ok(lines.every(line => line.endsWith(' skip')));
    assert.equal(fs.readFileSync(path.join(kept, 'answer.md'), 'utf8'), 'stored answer');
  } finally {
    cleanup();
  }
});
