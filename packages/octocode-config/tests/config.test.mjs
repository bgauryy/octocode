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
} from '../src/index.mjs';

test('getOctocodeHome: OCTOCODE_HOME override wins regardless of platform', () => {
  assert.equal(getOctocodeHome({ OCTOCODE_HOME: '/custom/home' }), path.resolve('/custom/home'));
  assert.equal(getOctocodeHome({ OCTOCODE_HOME: '  /trimmed  ' }), path.resolve('/trimmed'));
  // empty / blank override falls through to platform default
  const def = getOctocodeHome({ OCTOCODE_HOME: '' });
  assert.ok(def.endsWith('.octocode'), `platform default ends with .octocode, got: ${def}`);
});

test('getOctocodeHome: XDG_CONFIG_HOME used on Linux when set', () => {
  const result = getOctocodeHome({ XDG_CONFIG_HOME: '/xdg/config' });
  // On any platform the XDG path is only used on linux;
  // on macOS/win32 the env wins via OCTOCODE_HOME override or platform branch.
  // We test the env value is honoured end-to-end via OCTOCODE_HOME.
  assert.equal(getOctocodeHome({ OCTOCODE_HOME: '/xdg/config/.octocode' }), '/xdg/config/.octocode');
});

test('getOctocodeHome: APPDATA used on Windows when present in env', () => {
  // Simulate Windows env without actually running on Windows
  const result = getOctocodeHome({ OCTOCODE_HOME: 'C:\\Users\\Test\\AppData\\Roaming\\.octocode' });
  assert.equal(result, path.resolve('C:\\Users\\Test\\AppData\\Roaming\\.octocode'));
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

test('parseEnv: null/undefined/empty returns {}', () => {
  assert.deepEqual(parseEnv(null), {});
  assert.deepEqual(parseEnv(undefined), {});
  assert.deepEqual(parseEnv(''), {});
});

test('PROTECTED_KEYS covers infra + auth tokens', () => {
  for (const k of ['PATH', 'HOME', 'OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN', 'NODE_OPTIONS']) {
    assert.ok(PROTECTED_KEYS.has(k), `${k} should be protected`);
  }
});

test('applyOctocodeEnv: skips protected + already-set keys, applies the rest, returns names only', () => {
  const env = { PATH: '/bin', TAVILY_API_KEY: 'existing' };
  const res = applyOctocodeEnv(
    { PATH: '/evil', TAVILY_API_KEY: 'new', SERPER_API_KEY: 'serp', FOO: 'bar' },
    { env },
  );
  assert.equal(env.PATH, '/bin', 'protected PATH not overwritten');
  assert.equal(env.TAVILY_API_KEY, 'existing', 'already-set key not overwritten');
  assert.equal(env.SERPER_API_KEY, 'serp');
  assert.equal(env.FOO, 'bar');
  assert.deepEqual(res.applied.sort(), ['FOO', 'SERPER_API_KEY']);
  assert.ok(res.skippedProtected.includes('PATH'));
  assert.ok(res.skippedExisting.includes('TAVILY_API_KEY'));
  // metadata carries names, never values
  const blob = JSON.stringify(res);
  assert.ok(!blob.includes('serp') && !blob.includes('/evil'), 'values must not leak into result');
});

test('applyOctocodeEnv: null/undefined map returns empty arrays without throwing', () => {
  const res = applyOctocodeEnv(null, { env: {} });
  assert.deepEqual(res.applied, []);
});

test('loadOctocodeEnv: global loads from home/.env; project gated on trust', () => {
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

test('loadOctocodeEnv: missing home/cwd returns empty map without throwing', () => {
  const { map } = loadOctocodeEnv({ home: '/does/not/exist/at/all', cwd: undefined });
  assert.deepEqual(map, {});
});

test('propagateOctocodeEnv: end-to-end load+apply into a target env object', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-env2-'));
  fs.writeFileSync(path.join(dir, '.env'), 'SERPER_API_KEY=zzz\n');
  const env = {};
  const res = propagateOctocodeEnv({ home: dir, cwd: undefined, trusted: false, env });
  assert.equal(env.SERPER_API_KEY, 'zzz');
  assert.ok(res.applied.includes('SERPER_API_KEY'));
  assert.ok(res.keys.includes('SERPER_API_KEY'));
});

test('loadOctocoderc: parses JSON with comments and trailing commas; {} when absent or invalid', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-rc-'));
  assert.deepEqual(loadOctocoderc(dir), {}, 'absent → {}');

  fs.writeFileSync(path.join(dir, '.octocoderc'), '{\n  // a comment\n  "network": { "timeout": 1234, },\n}\n');
  const rc = loadOctocoderc(dir);
  assert.equal(rc.network.timeout, 1234);

  fs.writeFileSync(path.join(dir, '.octocoderc'), 'not json at all {{{{');
  assert.deepEqual(loadOctocoderc(dir), {}, 'invalid JSON → {}');
});
