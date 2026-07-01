import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {
  parseEnv,
  PROTECTED_KEYS,
  loadOctocodeEnv,
  applyOctocodeEnv,
  propagateOctocodeEnv,
  getOctocodeHome,
  loadOctocoderc,
} from '../src/env.js';

test('getOctocodeHome: OCTOCODE_HOME override wins; platform default otherwise', () => {
  assert.equal(getOctocodeHome({ OCTOCODE_HOME: '/custom/home' }), path.resolve('/custom/home'));
  const def = getOctocodeHome({});
  assert.ok(def.endsWith('.octocode'), 'platform default ends with .octocode');
});

test('loadOctocoderc: parses JSON with comments/trailing commas; {} when absent', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-rc-'));
  assert.deepEqual(loadOctocoderc(dir), {}, 'absent → {}');
  fs.writeFileSync(path.join(dir, '.octocoderc'), '{\n  // a comment\n  "network": { "timeout": 1234, },\n}\n');
  const rc = loadOctocoderc(dir);
  assert.equal(rc.network.timeout, 1234);
});

test('parseEnv: KEY=VALUE, comments, export prefix, quote stripping; ignores junk', () => {
  const map = parseEnv([
    '# comment',
    '',
    'TAVILY_API_KEY=tvly-abc',
    'export SERPER_API_KEY="serp-123"',
    "QUOTED='v a l'",
    'noequalsline',
    'EMPTY=',
  ].join('\n'));
  assert.equal(map.TAVILY_API_KEY, 'tvly-abc');
  assert.equal(map.SERPER_API_KEY, 'serp-123');
  assert.equal(map.QUOTED, 'v a l');
  assert.equal(map.EMPTY, '');
  assert.ok(!('noequalsline' in map));
});

test('applyOctocodeEnv: skips protected + already-set keys, applies the rest, returns names only', () => {
  const env = { PATH: '/bin', TAVILY_API_KEY: 'existing' };
  const res = applyOctocodeEnv(
    { PATH: '/evil', TAVILY_API_KEY: 'new', SERPER_API_KEY: 'serp', FOO: 'bar' },
    { env },
  );
  assert.equal(env.PATH, '/bin', 'protected PATH not overwritten');
  assert.equal(env.TAVILY_API_KEY, 'existing', 'already-set key not overwritten (env wins)');
  assert.equal(env.SERPER_API_KEY, 'serp', 'new key applied');
  assert.equal(env.FOO, 'bar');
  assert.deepEqual(res.applied.sort(), ['FOO', 'SERPER_API_KEY']);
  assert.ok(res.skippedProtected.includes('PATH'));
  assert.ok(res.skippedExisting.includes('TAVILY_API_KEY'));
  // metadata carries names, never values
  const blob = JSON.stringify(res);
  assert.ok(!blob.includes('serp') && !blob.includes('/evil'));
});

test('PROTECTED_KEYS covers infra + auth tokens', () => {
  for (const k of ['PATH', 'HOME', 'OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN']) {
    assert.ok(PROTECTED_KEYS.has(k), `${k} protected`);
  }
});

test('loadOctocodeEnv: project overrides global, and project is gated on trust', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-env-'));
  const home = path.join(dir, 'home');
  const cwd = path.join(dir, 'proj');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(path.join(cwd, '.octocode'), { recursive: true });
  fs.writeFileSync(path.join(home, '.env'), 'TAVILY_API_KEY=global\nGLOBAL_ONLY=g\n');
  fs.writeFileSync(path.join(cwd, '.octocode', '.env'), 'TAVILY_API_KEY=project\nPROJECT_ONLY=p\n');

  const untrusted = loadOctocodeEnv({ home, cwd, trusted: false });
  assert.equal(untrusted.map.TAVILY_API_KEY, 'global', 'untrusted: only global loaded');
  assert.ok(!('PROJECT_ONLY' in untrusted.map));

  const trusted = loadOctocodeEnv({ home, cwd, trusted: true });
  assert.equal(trusted.map.TAVILY_API_KEY, 'project', 'trusted: project overrides global');
  assert.equal(trusted.map.GLOBAL_ONLY, 'g');
  assert.equal(trusted.map.PROJECT_ONLY, 'p');
  assert.equal(trusted.sources.PROJECT_ONLY, 'project');
  assert.equal(trusted.sources.GLOBAL_ONLY, 'global');
});

test('propagateOctocodeEnv: end-to-end load+apply into a target env', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-env2-'));
  fs.writeFileSync(path.join(dir, '.env'), 'SERPER_API_KEY=zzz\n');
  const env = {};
  const res = propagateOctocodeEnv({ home: dir, cwd: undefined, trusted: false, env });
  assert.equal(env.SERPER_API_KEY, 'zzz');
  assert.ok(res.applied.includes('SERPER_API_KEY'));
});
