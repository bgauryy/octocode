import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

function fixture(t, { sibling = true, status = 0 } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-core-build-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const repo = path.join(root, 'octocode');
  const scripts = path.join(repo, 'skills-dev/octocode-dev/scripts');
  const core = path.join(root, 'octocode-mcp-host/packages/octocode-core');
  const bin = path.join(root, 'bin');
  fs.mkdirSync(path.join(scripts, 'lib'), { recursive: true });
  fs.mkdirSync(bin);
  fs.copyFileSync(new URL('./core-build.mjs', import.meta.url), path.join(scripts, 'core-build.mjs'));
  fs.writeFileSync(path.join(scripts, 'lib/workspaces.mjs'), `export const ROOT = ${JSON.stringify(repo)};`);
  const calls = path.join(root, 'calls.json');
  fs.writeFileSync(path.join(bin, 'yarn'), `#!/usr/bin/env node
import fs from 'node:fs';
fs.writeFileSync(${JSON.stringify(calls)}, JSON.stringify({ args: process.argv.slice(2), cwd: process.cwd() }));
process.exit(${status});
`, { mode: 0o755 });
  const put = (name, text, time) => {
    const file = path.join(core, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
    fs.utimesSync(file, time, time);
  };
  if (sibling) {
    put('src/schema.ts', 'export { tool } from "./tool.js";', 1000);
    put('src/tool.ts', 'export const tool = "before";', 1000);
    put('dist/schema.js', 'export { tool } from "./tool.js";', 2000);
    put('dist/tool.js', 'export const tool = "before";', 2000);
  }
  return {
    core, put,
    run() {
      const run = spawnSync(process.execPath, [path.join(scripts, 'core-build.mjs')], {
        encoding: 'utf8', env: { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH ?? ''}` },
      });
      return { ...run, call: fs.existsSync(calls) ? JSON.parse(fs.readFileSync(calls, 'utf8')) : null };
    },
  };
}

for (const scenario of ['deleted source', 'deleted source directory', 'missing output', 'preserved timestamps']) {
  test(`regeneration rebuilds core with ${scenario}`, t => {
    const f = fixture(t);
    if (scenario === 'deleted source') fs.unlinkSync(path.join(f.core, 'src/tool.ts'));
    if (scenario === 'deleted source directory') fs.rmSync(path.join(f.core, 'src'), { recursive: true });
    if (scenario === 'missing output') fs.unlinkSync(path.join(f.core, 'dist/schema.js'));
    if (scenario === 'preserved timestamps') f.put('src/tool.ts', 'export const tool = "after";', 1000);
    const run = f.run();
    assert.equal(run.status, 0, run.stderr);
    assert.deepEqual(run.call, { args: ['build'], cwd: fs.realpathSync(f.core) });
  });
}

test('a failed core build stops regeneration with its exit status', t => {
  const f = fixture(t, { status: 7 });
  f.put('src/tool.ts', 'export const tool = "after";', 3000);
  assert.equal(f.run().status, 7);
});

test('without a sibling checkout regeneration keeps using installed core', t => {
  const run = fixture(t, { sibling: false }).run();
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.call, null);
});
