import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { afterEach, beforeEach, test } from 'vitest';
import {
  resolveTool,
  registerGeneratedTool,
  runDynamicTool,
  listTools,
  deleteTool,
  recordUsage,
  readIndex,
  getRegistryDir,
  type ToolManifestEntry,
} from '../src/tools/dynamic-tools.js';

let dir: string;

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'calltool-test-'));
});
afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

const GOOD = {
  name: 'getCurrentTime',
  description: 'Return current time',
  keywords: ['time', 'clock', 'now', 'timezone'],
  capabilities: [] as ('net' | 'fs' | 'exec')[],
  reason: 'reusable in tests',
  sandboxed: true as boolean,
  deterministic: false as boolean,
  source: `export default async function ({ timezone = 'UTC' } = {}) { return { tool: 'getCurrentTime', timezone }; }`,
  test: `import fn from './tool.mjs';\nconst r = await fn({ timezone: 'UTC' });\nif (r.tool !== 'getCurrentTime') { console.error('bad'); process.exit(1); }\nprocess.exit(0);`,
};

function register(overrides: Partial<typeof GOOD> = {}, testTimeout?: number) {
  return registerGeneratedTool({ ...GOOD, ...overrides }, dir, testTimeout);
}

test('registerGeneratedTool registers a tool whose test passes', () => {
  fs.chmodSync(dir, 0o755);
  const res = register();
  assert.equal(res.ok, true);
  assert.ok(readIndex(dir).tools.getCurrentTime);
  if (process.platform !== 'win32') {
    const toolDir = path.join(dir, 'getCurrentTime');
    assert.equal(fs.statSync(dir).mode & 0o777, 0o700);
    assert.equal(fs.statSync(toolDir).mode & 0o777, 0o700);
    assert.equal(fs.statSync(path.join(dir, 'index.json')).mode & 0o777, 0o600);
    assert.equal(fs.statSync(path.join(toolDir, 'tool.mjs')).mode & 0o777, 0o600);
    assert.equal(fs.statSync(path.join(toolDir, 'tool.test.mjs')).mode & 0o777, 0o600);
  }
});

test('SANDBOX: the verification TEST run cannot see process.env secrets', () => {
  process.env.CALLTOOL_TEST_ENV_PROBE = 'leak-me';
  try {
    // The test fails (exit 1) if it can see the secret; passes only when the
    // test-run env is scrubbed. Registration ok proves scrubbing works.
    const res = register({
      name: 'envScrubProbe',
      capabilities: [],
      test: `if (process.env.CALLTOOL_TEST_ENV_PROBE) { console.error('leaked'); process.exit(1); }\nprocess.exit(0);`,
    });
    assert.equal(res.ok, true, 'secret must be scrubbed from the sandboxed test env');
  } finally {
    delete process.env.CALLTOOL_TEST_ENV_PROBE;
  }
});

test('getRegistryDir resolves under OCTOCODE_HOME', () => {
  const prev = process.env.OCTOCODE_HOME;
  process.env.OCTOCODE_HOME = dir;
  try {
    assert.equal(getRegistryDir(), path.join(dir, 'extension', 'dynamic-tools'));
  } finally {
    if (prev === undefined) delete process.env.OCTOCODE_HOME;
    else process.env.OCTOCODE_HOME = prev;
  }
});

test('resolveTool returns an exact O(1) hit', () => {
  register();
  const r = resolveTool('getCurrentTime', '', dir);
  assert.equal(r.hit, 'exact');
});

test('resolveTool matches a paraphrase via keyword overlap', () => {
  register();
  const r = resolveTool('whatTimeIsIt', 'get the current clock time now', dir);
  assert.equal(r.hit, 'keyword');
  if (r.hit === 'keyword') assert.ok(r.score >= 2);
});

test('resolveTool is a miss for unrelated requests', () => {
  register();
  const r = resolveTool('encodeBase64', 'encode bytes to base64', dir);
  assert.equal(r.hit, 'miss');
});

test('resolveTool matches a single-keyword tool on one overlapping token', () => {
  // Regression: with a fixed threshold of 2 a tool that declares one keyword
  // could never be resolved by keyword and was permanently invisible.
  register({ name: 'toSlug', keywords: ['slug'] });
  const r = resolveTool('makeSlug', 'turn a title into a slug', dir);
  assert.equal(r.hit, 'keyword');
  if (r.hit === 'keyword') assert.equal(r.entry.name, 'toSlug');
});

test('runDynamicTool executes in isolation and returns a structured result', () => {
  const reg = register();
  assert.ok(reg.ok);
  const run = runDynamicTool((reg as { entry: ToolManifestEntry }).entry, { timezone: 'Europe/Berlin' });
  assert.equal(run.ok, true);
  if (run.ok) assert.deepEqual(run.result, { tool: 'getCurrentTime', timezone: 'Europe/Berlin' });
});

test('verification gate rejects a tool whose test fails and leaves no index entry', () => {
  const res = register({
    name: 'brokenTool',
    test: `import fn from './tool.mjs';\nconst r = await fn();\nif (!r.mustHaveThis) { console.error('missing'); process.exit(1); }`,
  });
  assert.equal(res.ok, false);
  if (!res.ok) assert.equal(res.reason, 'test-failed');
  assert.equal(readIndex(dir).tools.brokenTool, undefined);
});

test('verification gate times out a hanging test', () => {
  const res = register({ name: 'hangTest', test: `while (true) {}` }, 500);
  assert.equal(res.ok, false);
  if (!res.ok) assert.equal(res.reason, 'test-timeout');
});

test('invalid tool names are rejected', () => {
  const res = register({ name: '1bad name!' });
  assert.equal(res.ok, false);
  if (!res.ok) assert.equal(res.reason, 'invalid-name');
});

test('registration without a reason is rejected', () => {
  const res = register({ name: 'noReasonTool', reason: '   ' });
  assert.equal(res.ok, false);
  if (!res.ok) assert.equal(res.reason, 'no-reason');
});

test('deleteTool removes the entry and directory', () => {
  const reg = register();
  assert.ok(reg.ok);
  assert.equal(deleteTool('getCurrentTime', dir), true);
  assert.equal(readIndex(dir).tools.getCurrentTime, undefined);
  assert.equal(deleteTool('getCurrentTime', dir), false);
});

test('checksum mismatch blocks execution of a tampered tool', () => {
  const reg = register();
  assert.ok(reg.ok);
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  fs.writeFileSync(entry.entry, GOOD.source + '\n// tampered');
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'checksum-mismatch');
});

test('missing entry file yields not-found', () => {
  const reg = register();
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  fs.rmSync(entry.entry);
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'not-found');
});

test('SANDBOX: a tool with no net capability cannot reach the network', () => {
  const reg = register({
    name: 'sneakyNet',
    capabilities: [],
    source: `export default async () => { await fetch('http://127.0.0.1:9/'); return { ok: true }; };`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false); // ERR_ACCESS_DENIED from the permission model
  if (!run.ok) assert.equal(run.reason, 'exec-failed');
});

test('SANDBOX: a tool with no fs capability cannot read arbitrary files', () => {
  const reg = register({
    name: 'sneakyRead',
    capabilities: [],
    source: `import fs from 'node:fs'; export default async () => ({ n: fs.readFileSync('/etc/hosts','utf8').length });`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'exec-failed');
});

test('SANDBOX: process.env secrets are scrubbed from a sandboxed tool', () => {
  process.env.CALLTOOL_SECRET_PROBE = 'top-secret';
  try {
    const reg = register({
      name: 'envProbe',
      capabilities: [],
      source: `export default async () => ({ leaked: process.env.CALLTOOL_SECRET_PROBE ?? null });`,
      test: `process.exit(0);`,
    });
    const entry = (reg as { entry: ToolManifestEntry }).entry;
    const run = runDynamicTool(entry, {});
    assert.equal(run.ok, true);
    if (run.ok) assert.deepEqual(run.result, { leaked: null });
  } finally {
    delete process.env.CALLTOOL_SECRET_PROBE;
  }
});

test('SANDBOX: a non-sandboxed tool runs with inherited env (opt-in trust)', () => {
  process.env.CALLTOOL_TRUST_PROBE = 'visible';
  try {
    const reg = register({
      name: 'trustedProbe',
      sandboxed: false,
      capabilities: [],
      source: `export default async () => ({ seen: process.env.CALLTOOL_TRUST_PROBE ?? null });`,
      test: `process.exit(0);`,
    });
    const entry = (reg as { entry: ToolManifestEntry }).entry;
    assert.equal(entry.sandboxed, false);
    const run = runDynamicTool(entry, {});
    assert.equal(run.ok, true);
    if (run.ok) assert.deepEqual(run.result, { seen: 'visible' });
  } finally {
    delete process.env.CALLTOOL_TRUST_PROBE;
  }
});

test('undeclared capability is denied, approved capability runs', () => {
  const reg = register({
    name: 'netTool',
    capabilities: ['net'],
    source: `export default async () => ({ ok: true });`,
    test: `import fn from './tool.mjs';\nawait fn();\nprocess.exit(0);`,
  });
  assert.ok(reg.ok);
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const denied = runDynamicTool(entry, {}, { allow: [] });
  assert.equal(denied.ok, false);
  if (!denied.ok) assert.equal(denied.reason, 'capability-denied:net');
  const allowed = runDynamicTool(entry, {}, { allow: ['net'] });
  assert.equal(allowed.ok, true);
});

test('runaway tool is killed by the execution timeout', () => {
  const reg = register({
    name: 'loopTool',
    source: `export default async () => { while (true) {} };`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const t0 = Date.now();
  const run = runDynamicTool(entry, {}, { timeoutMs: 500 });
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'exec-timeout');
  assert.ok(Date.now() - t0 < 5000);
});

test('a throwing tool yields exec-failed', () => {
  const reg = register({
    name: 'throwTool',
    source: `export default async () => { throw new Error('boom'); };`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'exec-failed');
});

test('non-JSON stdout yields bad-output', () => {
  const reg = register({
    name: 'noisyTool',
    source: `export default async () => { console.log('side channel noise'); return { ok: 1 }; };`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'bad-output');
});

test('re-registration bumps the version and preserves createdAt', () => {
  const first = register();
  assert.ok(first.ok);
  const created = (first as { entry: ToolManifestEntry }).entry.createdAt;
  const second = register({ description: 'v2' });
  assert.ok(second.ok);
  if (second.ok) {
    assert.equal(second.entry.version, 2);
    assert.equal(second.entry.createdAt, created);
  }
});

test('listTools returns registered entries', () => {
  register();
  register({ name: 'toSlug', keywords: ['slug'] });
  assert.equal(listTools(dir).length, 2);
});

test('recordUsage increments call and failure counters', () => {
  register();
  recordUsage('getCurrentTime', true, dir);
  recordUsage('getCurrentTime', false, dir);
  const entry = readIndex(dir).tools.getCurrentTime;
  assert.equal(entry.stats.calls, 2);
  assert.equal(entry.stats.failures, 1);
  assert.ok(entry.stats.lastUsedAt);
});

test('recordUsage on an unknown tool is a no-op', () => {
  register();
  recordUsage('does-not-exist', true, dir);
  assert.equal(readIndex(dir).tools.getCurrentTime.stats.calls, 0);
});

test('ROLLBACK: a failed enhance restores the previous good tool (no soft-broken state)', () => {
  const v1 = register();
  assert.ok(v1.ok);
  // Re-register (enhance) with a FAILING test → must roll back to v1.
  const bad = register({ test: `process.exit(1);` });
  assert.equal(bad.ok, false);
  // The still-indexed v1 entry must run cleanly (files restored, checksum matches).
  const entry = readIndex(dir).tools.getCurrentTime;
  assert.equal(entry.version, 1);
  const run = runDynamicTool(entry, { timezone: 'UTC' });
  assert.equal(run.ok, true);
});

test('STDIN: large metadata (beyond argv limits) is delivered via stdin', () => {
  const reg = register({
    name: 'echoBig',
    source: `export default async (m) => ({ len: (m.blob || '').length });`,
    test: `import fn from './tool.mjs';\nconst r = await fn({ blob: 'abc' });\nif (r.len !== 3) process.exit(1);\nprocess.exit(0);`,
  });
  assert.ok(reg.ok);
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const blob = 'x'.repeat(300_000); // exceeds typical argv single-arg limits
  const run = runDynamicTool(entry, { blob });
  assert.equal(run.ok, true);
  if (run.ok) assert.deepEqual(run.result, { len: 300_000 });
});

test('HARDENING: eval / code-generation-from-strings is blocked in the sandbox', () => {
  const reg = register({
    name: 'evalTool',
    source: `export default async () => ({ v: eval('1+1') });`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const run = runDynamicTool(entry, {});
  assert.equal(run.ok, false);
  if (!run.ok) assert.equal(run.reason, 'exec-failed');
});

test('LOCK: the registry lock dir is released after a mutating op', () => {
  register();
  assert.equal(fs.existsSync(path.join(dir, '.index.lock')), false);
  deleteTool('getCurrentTime', dir);
  assert.equal(fs.existsSync(path.join(dir, '.index.lock')), false);
});

test('CACHE: a deterministic, capability-free tool memoizes results by metadata', () => {
  const reg = register({
    name: 'randPure',
    deterministic: true,
    capabilities: [],
    source: `export default async ({ seed = 0 }) => ({ v: Math.random(), seed });`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const first = runDynamicTool(entry, { seed: 1 });
  const second = runDynamicTool(entry, { seed: 1 });
  assert.equal(first.ok && first.cached, false, 'first run executes');
  assert.equal(second.ok && second.cached, true, 'second run is served from cache');
  if (first.ok && second.ok) assert.deepEqual(second.result, first.result, 'cached result is identical');
  // Different metadata is a cache miss (executes again).
  const other = runDynamicTool(entry, { seed: 2 });
  assert.equal(other.ok && other.cached, false, 'different metadata misses the cache');
});

test('CACHE: a non-deterministic tool is never memoized', () => {
  const reg = register({
    name: 'randImpure',
    deterministic: false,
    source: `export default async () => ({ v: Math.random() });`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const r1 = runDynamicTool(entry, {});
  const r2 = runDynamicTool(entry, {});
  assert.equal(r1.ok && r1.cached, false);
  assert.equal(r2.ok && r2.cached, false);
});

test('CACHE: a tool with capabilities is never memoized (may have side effects)', () => {
  const reg = register({
    name: 'netPure',
    deterministic: true,
    capabilities: ['net'],
    source: `export default async () => ({ v: 1 });`,
    test: `process.exit(0);`,
  });
  const entry = (reg as { entry: ToolManifestEntry }).entry;
  const r1 = runDynamicTool(entry, {}, { allow: ['net'] });
  const r2 = runDynamicTool(entry, {}, { allow: ['net'] });
  assert.equal(r1.ok && r1.cached, false);
  assert.equal(r2.ok && r2.cached, false);
});

test('CACHE: re-registering a new version busts the cache', () => {
  register({ name: 'verPure', deterministic: true, source: `export default async () => ({ v: Math.random() });`, test: `process.exit(0);` });
  const v1 = readIndex(dir).tools.verPure;
  const a = runDynamicTool(v1, {});
  const b = runDynamicTool(v1, {});
  assert.equal(b.ok && b.cached, true, 'v1 cached');
  register({ name: 'verPure', deterministic: true, source: `export default async () => ({ v: 42 });`, test: `process.exit(0);` });
  const v2 = readIndex(dir).tools.verPure;
  assert.equal(v2.version, 2);
  const c = runDynamicTool(v2, {});
  assert.equal(c.ok && c.cached, false, 'new version misses the old cache');
  if (c.ok) assert.deepEqual(c.result, { v: 42 });
  void a;
});

test('a corrupt index reads as empty rather than throwing', () => {
  register();
  fs.writeFileSync(path.join(dir, 'index.json'), '{ not json');
  assert.deepEqual(readIndex(dir), { version: 1, tools: {} });
});
