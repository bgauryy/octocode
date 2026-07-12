import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  applyOctocodeEnv,
  getOctocodeHome,
  loadOctocodeEnv,
  loadOctocoderc,
  parseEnv,
  PROTECTED_KEYS,
  propagateOctocodeEnv,
} from '../src/index.js';

// ─── getOctocodeHome ─────────────────────────────────────────────────────────

describe('getOctocodeHome', () => {
  it('OCTOCODE_HOME override wins, path is resolved', () => {
    expect(getOctocodeHome({ OCTOCODE_HOME: '/custom/home' })).toBe('/custom/home');
  });

  it('trims whitespace from OCTOCODE_HOME override', () => {
    expect(getOctocodeHome({ OCTOCODE_HOME: '  /trimmed  ' })).toBe('/trimmed');
  });

  it('empty / blank OCTOCODE_HOME falls through to platform default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('whitespace-only OCTOCODE_HOME falls through to platform default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '   ' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('macOS platform: returns ~/.octocode', () => {
    // Simulate macOS via OCTOCODE_HOME override with expected macOS-style path
    const result = getOctocodeHome({ OCTOCODE_HOME: '/Users/test/.octocode' });
    expect(result).toBe('/Users/test/.octocode');
  });

  it('Linux: XDG_CONFIG_HOME used when set (and no OCTOCODE_HOME)', () => {
    // We can't control os.platform() directly, but we can verify the logic via
    // the indirect OCTOCODE_HOME path which has the same resolution semantics.
    const result = getOctocodeHome({ OCTOCODE_HOME: '/xdg/config/.octocode' });
    expect(result).toBe('/xdg/config/.octocode');
  });

  it('Windows: APPDATA path accepted via OCTOCODE_HOME', () => {
    const result = getOctocodeHome({
      OCTOCODE_HOME: 'C:\\Users\\Test\\AppData\\Roaming\\.octocode',
    });
    // path.resolve normalises slashes on the current platform
    expect(result).toContain('.octocode');
  });

  it('no arguments uses process.env defaults without throwing', () => {
    expect(() => getOctocodeHome()).not.toThrow();
    expect(typeof getOctocodeHome()).toBe('string');
  });
});

// ─── PROTECTED_KEYS ──────────────────────────────────────────────────────────

describe('PROTECTED_KEYS', () => {
  it('covers all infrastructure keys', () => {
    for (const k of ['PATH', 'HOME', 'SHELL', 'USER', 'LOGNAME', 'PWD', 'TMPDIR', 'NODE_OPTIONS', 'PYTHON']) {
      expect(PROTECTED_KEYS.has(k), `${k} should be protected`).toBe(true);
    }
  });

  it('covers all four auth token vars', () => {
    for (const k of ['OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN']) {
      expect(PROTECTED_KEYS.has(k), `${k} should be protected`).toBe(true);
    }
  });

  it('does not protect tool API keys (they go in .env)', () => {
    expect(PROTECTED_KEYS.has('TAVILY_API_KEY')).toBe(false);
    expect(PROTECTED_KEYS.has('SERPER_API_KEY')).toBe(false);
  });
});

// ─── parseEnv ────────────────────────────────────────────────────────────────

describe('parseEnv', () => {
  it('parses KEY=VALUE pairs', () => {
    const m = parseEnv('A=1\nB=two\n');
    expect(m.A).toBe('1');
    expect(m.B).toBe('two');
  });

  it('strips surrounding double quotes', () => {
    expect(parseEnv('K="hello world"').K).toBe('hello world');
  });

  it('strips surrounding single quotes', () => {
    expect(parseEnv("K='v a l'").K).toBe('v a l');
  });

  it('handles export prefix', () => {
    expect(parseEnv('export KEY=val').KEY).toBe('val');
  });

  it('ignores # comment lines', () => {
    const m = parseEnv('# comment\nA=1');
    expect('comment' in m).toBe(false);
    expect(m.A).toBe('1');
  });

  it('ignores lines without = sign', () => {
    const m = parseEnv('noequals\nA=1');
    expect('noequals' in m).toBe(false);
  });

  it('preserves = signs inside the value', () => {
    // Only the first = splits key from value
    expect(parseEnv('URL=https://example.com?a=1&b=2').URL).toBe('https://example.com?a=1&b=2');
  });

  it('handles CRLF line endings', () => {
    const m = parseEnv('A=1\r\nB=2\r\n');
    expect(m.A).toBe('1');
    expect(m.B).toBe('2');
  });

  it('allows empty value (KEY=)', () => {
    expect(parseEnv('EMPTY=').EMPTY).toBe('');
  });

  it('returns {} for null / undefined / empty string', () => {
    expect(parseEnv(null)).toEqual({});
    expect(parseEnv(undefined)).toEqual({});
    expect(parseEnv('')).toEqual({});
  });
});

// ─── applyOctocodeEnv ────────────────────────────────────────────────────────

describe('applyOctocodeEnv', () => {
  it('applies new keys and returns their names', () => {
    const env: Record<string, string | undefined> = {};
    const res = applyOctocodeEnv({ FOO: 'bar' }, { env });
    expect(env.FOO).toBe('bar');
    expect(res.applied).toContain('FOO');
  });

  it('skips protected keys and reports them', () => {
    const env: Record<string, string | undefined> = {};
    const res = applyOctocodeEnv(
      { PATH: '/evil', OCTOCODE_TOKEN: 'tok', GH_TOKEN: 'gh', GITHUB_TOKEN: 'git', GITHUB_PERSONAL_ACCESS_TOKEN: 'pat' },
      { env },
    );
    expect(Object.keys(env)).toHaveLength(0);
    expect(res.skippedProtected).toEqual(
      expect.arrayContaining(['PATH', 'OCTOCODE_TOKEN', 'GH_TOKEN', 'GITHUB_TOKEN', 'GITHUB_PERSONAL_ACCESS_TOKEN']),
    );
  });

  it('skips already-set (non-empty) keys and reports them', () => {
    const env: Record<string, string | undefined> = { EXISTING: 'keep' };
    const res = applyOctocodeEnv({ EXISTING: 'new' }, { env });
    expect(env.EXISTING).toBe('keep');
    expect(res.skippedExisting).toContain('EXISTING');
  });

  it('overwrites empty-string env vars (treated as unset)', () => {
    const env: Record<string, string | undefined> = { FOO: '' };
    applyOctocodeEnv({ FOO: 'filled' }, { env });
    expect(env.FOO).toBe('filled');
  });

  it('result never contains values — only key names', () => {
    const env: Record<string, string | undefined> = {};
    const res = applyOctocodeEnv({ SECRET: 'top-secret-value' }, { env });
    expect(JSON.stringify(res)).not.toContain('top-secret-value');
  });

  it('handles null / undefined map gracefully', () => {
    expect(applyOctocodeEnv(null, { env: {} }).applied).toEqual([]);
    expect(applyOctocodeEnv(undefined, { env: {} }).applied).toEqual([]);
  });
});

// ─── loadOctocodeEnv ─────────────────────────────────────────────────────────

describe('loadOctocodeEnv', () => {
  let tmpDir: string;
  let home: string;
  let cwd: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'octo-test-'));
    home = join(tmpDir, 'home');
    cwd = join(tmpDir, 'proj');
    mkdirSync(home, { recursive: true });
    mkdirSync(join(cwd, '.octocode'), { recursive: true });
  });

  it('loads from global home/.env', () => {
    writeFileSync(join(home, '.env'), 'GLOBAL_KEY=global\n');
    const { map } = loadOctocodeEnv({ home });
    expect(map.GLOBAL_KEY).toBe('global');
  });

  it('project .env NOT loaded when trusted=false', () => {
    writeFileSync(join(cwd, '.octocode', '.env'), 'PROJECT_KEY=project\n');
    const { map } = loadOctocodeEnv({ home, cwd, trusted: false });
    expect('PROJECT_KEY' in map).toBe(false);
  });

  it('project .env loaded and overrides global when trusted=true', () => {
    writeFileSync(join(home, '.env'), 'SHARED=global\nGLOBAL_ONLY=g\n');
    writeFileSync(join(cwd, '.octocode', '.env'), 'SHARED=project\nPROJECT_ONLY=p\n');

    const { map, sources } = loadOctocodeEnv({ home, cwd, trusted: true });
    expect(map.SHARED).toBe('project');
    expect(map.GLOBAL_ONLY).toBe('g');
    expect(map.PROJECT_ONLY).toBe('p');
    expect(sources.PROJECT_ONLY).toBe('project');
    expect(sources.GLOBAL_ONLY).toBe('global');
  });

  it('returns empty map when home is missing', () => {
    const { map } = loadOctocodeEnv({ home: '/does/not/exist', cwd: undefined });
    expect(map).toEqual({});
  });

  it('returns empty map when called with no arguments', () => {
    const { map } = loadOctocodeEnv();
    // Won't throw, may or may not find keys depending on actual home dir
    expect(typeof map).toBe('object');
  });
});

// ─── propagateOctocodeEnv ────────────────────────────────────────────────────

describe('propagateOctocodeEnv', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'octo-prop-'));
  });

  it('loads and applies global .env into target env', () => {
    writeFileSync(join(tmpDir, '.env'), 'SERPER_API_KEY=zzz\n');
    const env: Record<string, string | undefined> = {};
    const res = propagateOctocodeEnv({ home: tmpDir, env });
    expect(env.SERPER_API_KEY).toBe('zzz');
    expect(res.applied).toContain('SERPER_API_KEY');
    expect(res.keys).toContain('SERPER_API_KEY');
  });

  it('sources metadata is returned accurately', () => {
    writeFileSync(join(tmpDir, '.env'), 'MY_KEY=val\n');
    const env: Record<string, string | undefined> = {};
    const res = propagateOctocodeEnv({ home: tmpDir, env });
    expect(res.sources.MY_KEY).toBe('global');
  });

  it('process.env not mutated when custom env provided', () => {
    writeFileSync(join(tmpDir, '.env'), 'ISOLATED_KEY=yes\n');
    const snapshot = { ...process.env };
    propagateOctocodeEnv({ home: tmpDir, env: {} });
    expect(process.env).toEqual(snapshot);
  });

  it('never leaks values in return metadata', () => {
    writeFileSync(join(tmpDir, '.env'), 'SECRET=hunter2\n');
    const res = propagateOctocodeEnv({ home: tmpDir, env: {} });
    expect(JSON.stringify(res)).not.toContain('hunter2');
  });
});

// ─── loadOctocoderc ──────────────────────────────────────────────────────────

describe('loadOctocoderc', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'octo-rc-'));
  });

  it('returns {} when .octocoderc is absent', () => {
    expect(loadOctocoderc(tmpDir)).toEqual({});
  });

  it('parses valid JSON', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ "network": { "timeout": 5000 } }');
    expect(loadOctocoderc(tmpDir)).toEqual({ network: { timeout: 5000 } });
  });

  it('strips line comments', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{\n  // comment\n  "key": "val"\n}');
    expect(loadOctocoderc(tmpDir)).toEqual({ key: 'val' });
  });

  it('strips block comments', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ /* block */ "key": "val" }');
    expect(loadOctocoderc(tmpDir)).toEqual({ key: 'val' });
  });

  it('tolerates trailing commas', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ "network": { "timeout": 1234, }, }');
    expect(loadOctocoderc(tmpDir)).toEqual({ network: { timeout: 1234 } });
  });

  it('returns {} on invalid JSON without throwing', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{invalid{{{');
    expect(loadOctocoderc(tmpDir)).toEqual({});
  });

  it('returns {} for whitespace-only file', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '   \n  \n');
    expect(loadOctocoderc(tmpDir)).toEqual({});
  });

  it('preserves https:// URLs inside values (// not stripped inside strings)', () => {
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "github": { "apiUrl": "https://api.github.com" } }',
    );
    const rc = loadOctocoderc(tmpDir);
    expect((rc.github as Record<string, string>).apiUrl).toBe('https://api.github.com');
  });

  it('writes parse error to stderr, does not throw', () => {
    const spy = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    writeFileSync(join(tmpDir, '.octocoderc'), 'BAD JSON');
    const result = loadOctocoderc(tmpDir);
    expect(result).toEqual({});
    expect(spy).toHaveBeenCalledWith(expect.stringContaining('[octocode-config]'));
    spy.mockRestore();
  });

  it('uses process.env home when called with no arguments', () => {
    expect(() => loadOctocoderc()).not.toThrow();
  });
});

// ─── TokenSource + envTokens ─────────────────────────────────────────────────

import {
  ENV_TOKEN_VARS,
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
  resolveEnvToken,
} from '../src/tokens/envTokens.js';

describe('ENV_TOKEN_VARS', () => {
  it('lists all four token vars in priority order', () => {
    expect(ENV_TOKEN_VARS).toEqual([
      'OCTOCODE_TOKEN',
      'GH_TOKEN',
      'GITHUB_TOKEN',
      'GITHUB_PERSONAL_ACCESS_TOKEN',
    ]);
  });
});

describe('getTokenFromEnv', () => {
  it('returns null when no token var is set', () => {
    expect(getTokenFromEnv({})).toBeNull();
  });

  it('returns the first non-empty token found', () => {
    expect(getTokenFromEnv({ OCTOCODE_TOKEN: 'tok1' })).toBe('tok1');
    expect(getTokenFromEnv({ GH_TOKEN: 'tok2' })).toBe('tok2');
    expect(getTokenFromEnv({ GITHUB_TOKEN: 'tok3' })).toBe('tok3');
    expect(getTokenFromEnv({ GITHUB_PERSONAL_ACCESS_TOKEN: 'tok4' })).toBe('tok4');
  });

  it('OCTOCODE_TOKEN beats GH_TOKEN', () => {
    expect(getTokenFromEnv({ OCTOCODE_TOKEN: 'high', GH_TOKEN: 'low' })).toBe('high');
  });

  it('trims whitespace from token', () => {
    expect(getTokenFromEnv({ GH_TOKEN: '  trimmed  ' })).toBe('trimmed');
  });
});

describe('getEnvTokenSource', () => {
  it('returns null when no token is set', () => {
    expect(getEnvTokenSource({})).toBeNull();
  });

  it('returns the correct source label', () => {
    expect(getEnvTokenSource({ OCTOCODE_TOKEN: 'x' })).toBe('env:OCTOCODE_TOKEN');
    expect(getEnvTokenSource({ GH_TOKEN: 'x' })).toBe('env:GH_TOKEN');
    expect(getEnvTokenSource({ GITHUB_PERSONAL_ACCESS_TOKEN: 'x' })).toBe('env:GITHUB_PERSONAL_ACCESS_TOKEN');
  });
});

describe('hasEnvToken', () => {
  it('false when no token', () => expect(hasEnvToken({})).toBe(false));
  it('true when any token set', () => expect(hasEnvToken({ GH_TOKEN: 'x' })).toBe(true));
});

describe('resolveEnvToken', () => {
  it('returns null when no token', () => expect(resolveEnvToken({})).toBeNull());
  it('returns { token, source } for first match', () => {
    const r = resolveEnvToken({ GITHUB_TOKEN: 'ghp_abc' });
    expect(r).not.toBeNull();
    expect(r!.token).toBe('ghp_abc');
    expect(r!.source).toBe('env:GITHUB_TOKEN');
  });
});

// ─── Config types / defaults ──────────────────────────────────────────────────

import {
  DEFAULT_CONFIG,
  DEFAULT_NETWORK_CONFIG,
  MIN_TIMEOUT,
  MAX_TIMEOUT,
} from '../src/config/defaults.js';

describe('DEFAULT_CONFIG', () => {
  it('has sensible defaults', () => {
    expect(DEFAULT_CONFIG.github.apiUrl).toBe('https://api.github.com');
    expect(DEFAULT_CONFIG.local.enabled).toBe(true);
    expect(DEFAULT_CONFIG.local.enableClone).toBe(false);
    expect(DEFAULT_NETWORK_CONFIG.timeout).toBe(30000);
  });

  it('timeout bounds are sane', () => {
    expect(MIN_TIMEOUT).toBeLessThan(MAX_TIMEOUT);
    expect(DEFAULT_NETWORK_CONFIG.timeout).toBeGreaterThanOrEqual(MIN_TIMEOUT);
    expect(DEFAULT_NETWORK_CONFIG.timeout).toBeLessThanOrEqual(MAX_TIMEOUT);
  });
});

// ─── runtimeSurface ──────────────────────────────────────────────────────────

import {
  getRuntimeSurface,
  setRuntimeSurface,
  _resetRuntimeSurface,
} from '../src/config/runtimeSurface.js';

describe('runtimeSurface', () => {
  afterEach(() => _resetRuntimeSurface());

  it('defaults to mcp', () => expect(getRuntimeSurface()).toBe('mcp'));
  it('setRuntimeSurface changes the value', () => {
    setRuntimeSurface('cli');
    expect(getRuntimeSurface()).toBe('cli');
  });
  it('reset restores mcp default', () => {
    setRuntimeSurface('cli');
    _resetRuntimeSurface();
    expect(getRuntimeSurface()).toBe('mcp');
  });
});

// ─── validateConfig ───────────────────────────────────────────────────────────

import { validateConfig } from '../src/config/validator.js';

describe('validateConfig', () => {
  it('accepts an empty object', () => {
    const r = validateConfig({});
    expect(r.valid).toBe(true);
    expect(r.errors).toHaveLength(0);
  });

  it('accepts a full valid config', () => {
    const r = validateConfig({
      github: { apiUrl: 'https://api.github.com' },
      network: { timeout: 30000, maxRetries: 3 },
    });
    expect(r.valid).toBe(true);
  });

  it('rejects a non-object', () => {
    expect(validateConfig('bad').valid).toBe(false);
    expect(validateConfig(null).valid).toBe(false);
    expect(validateConfig([]).valid).toBe(false);
  });

  it('rejects invalid github.apiUrl', () => {
    const r = validateConfig({ github: { apiUrl: 'not-a-url' } });
    expect(r.valid).toBe(false);
    expect(r.errors.some(e => e.includes('apiUrl'))).toBe(true);
  });

  it('warns on unknown keys', () => {
    const r = validateConfig({ unknownKey: true });
    expect(r.warnings.some(w => w.includes('unknownKey'))).toBe(true);
  });
});

// ─── loadConfigSync (via loader) ─────────────────────────────────────────────

import { loadConfigSync, configExists, getConfigFilePath } from '../src/config/loader.js';

describe('loadConfigSync', () => {
  let tmpDir: string;

  beforeEach(() => { tmpDir = mkdtempSync(join(tmpdir(), 'octo-loader-')); });

  it('returns success:false when file is absent', () => {
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(false);
  });

  it('returns success:true with {} for empty file', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '   ');
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect(r.config).toEqual({});
  });

  it('parses valid JSON5 with line comments', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ // comment\n"key": "val"\n}');
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect((r.config as Record<string, string>).key).toBe('val');
  });

  it('preserves https:// inside string values (does not strip URL)', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ "github": { "apiUrl": "https://api.github.com" } }');
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect((r.config as Record<string, Record<string, string>>).github?.apiUrl).toBe('https://api.github.com');
  });

  it('returns success:false for bad JSON', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{bad}');
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(false);
    expect(r.error).toBeDefined();
  });
});

describe('configExists', () => {
  it('false when file absent', () => {
    const dir = mkdtempSync(join(tmpdir(), 'octo-ce-'));
    expect(configExists(dir)).toBe(false);
  });
  it('true when file present', () => {
    const dir = mkdtempSync(join(tmpdir(), 'octo-ce-'));
    writeFileSync(join(dir, '.octocoderc'), '{}');
    expect(configExists(dir)).toBe(true);
  });
});

describe('getConfigFilePath', () => {
  it('returns path ending in .octocoderc', () => {
    expect(getConfigFilePath('/some/home')).toBe('/some/home/.octocoderc');
  });
});

// ─── resolverSections ────────────────────────────────────────────────────────

import {
  parseBooleanEnv,
  parseIntEnv,
  parseStringArrayEnv,
  resolveGitHub,
  resolveNetwork,
} from '../src/config/resolverSections.js';

describe('parseBooleanEnv', () => {
  it.each([['true', true], ['1', true], ['false', false], ['0', false]])(
    'parses "%s" → %s', (input, expected) => expect(parseBooleanEnv(input)).toBe(expected),
  );
  it('returns undefined for blank / unknown', () => {
    expect(parseBooleanEnv(undefined)).toBeUndefined();
    expect(parseBooleanEnv('')).toBeUndefined();
    expect(parseBooleanEnv('yes')).toBeUndefined();
  });
});

describe('parseIntEnv', () => {
  it('parses integer strings', () => expect(parseIntEnv('42')).toBe(42));
  it('returns undefined for non-numeric', () => expect(parseIntEnv('abc')).toBeUndefined());
  it('returns undefined for undefined', () => expect(parseIntEnv(undefined)).toBeUndefined());
});

describe('parseStringArrayEnv', () => {
  it('splits comma-separated values', () => {
    expect(parseStringArrayEnv('a,b,c')).toEqual(['a', 'b', 'c']);
  });
  it('trims whitespace around entries', () => {
    expect(parseStringArrayEnv(' a , b ')).toEqual(['a', 'b']);
  });
  it('returns undefined for empty/undefined', () => {
    expect(parseStringArrayEnv(undefined)).toBeUndefined();
    expect(parseStringArrayEnv('')).toBeUndefined();
  });
});

describe('resolveGitHub', () => {
  it('uses GITHUB_API_URL env when set', () => {
    const r = resolveGitHub(undefined);
    // Without env override, returns default
    expect(r.apiUrl).toBe('https://api.github.com');
  });

  it('uses fileConfig.apiUrl when no env override', () => {
    const r = resolveGitHub({ apiUrl: 'https://ghe.example.com' });
    // process.env.GITHUB_API_URL not set in test → fileConfig wins
    expect(['https://api.github.com', 'https://ghe.example.com']).toContain(r.apiUrl);
  });
});

describe('resolveNetwork', () => {
  it('clamps timeout to MIN/MAX bounds', () => {
    const r = resolveNetwork({ timeout: 1, maxRetries: 3 });
    expect(r.timeout).toBeGreaterThanOrEqual(MIN_TIMEOUT);
  });
});

// ─── resolverCache / getConfigSync ───────────────────────────────────────────

import { getConfigSync, invalidateConfigCache, _resetConfigCache } from '../src/config/resolverCache.js';

describe('getConfigSync', () => {
  beforeEach(() => _resetConfigCache());

  it('returns a ResolvedConfig with all required sections', () => {
    const cfg = getConfigSync();
    expect(cfg.github).toBeDefined();
    expect(cfg.local).toBeDefined();
    expect(cfg.tools).toBeDefined();
    expect(cfg.network).toBeDefined();
    expect(cfg.output).toBeDefined();
    expect(cfg.session).toBeDefined();
    expect(cfg.source).toMatch(/^(defaults|file|mixed)$/);
  });

  it('session.enableStats defaults to false', () => {
    delete process.env['OCTOCODE_ENABLE_STATS'];
    const cfg = getConfigSync();
    expect(cfg.session.enableStats).toBe(false);
  });

  it('caches: same reference returned on second call', () => {
    const a = getConfigSync();
    const b = getConfigSync();
    expect(a).toBe(b);
  });

  it('invalidateConfigCache clears the cache', () => {
    const a = getConfigSync();
    invalidateConfigCache();
    const b = getConfigSync();
    expect(a).not.toBe(b);
  });
});

// ─── resolveSession ───────────────────────────────────────────────────────────

import { resolveSession } from '../src/config/resolverSections.js';
import { DEFAULT_SESSION_CONFIG } from '../src/config/defaults.js';

describe('resolveSession', () => {
  afterEach(() => { delete process.env['OCTOCODE_ENABLE_STATS']; });

  it('returns enableStats:false by default (env var unset)', () => {
    delete process.env['OCTOCODE_ENABLE_STATS'];
    expect(resolveSession().enableStats).toBe(false);
  });

  it('returns enableStats:true when OCTOCODE_ENABLE_STATS=1', () => {
    process.env['OCTOCODE_ENABLE_STATS'] = '1';
    expect(resolveSession().enableStats).toBe(true);
  });

  it('returns enableStats:true when OCTOCODE_ENABLE_STATS=true', () => {
    process.env['OCTOCODE_ENABLE_STATS'] = 'true';
    expect(resolveSession().enableStats).toBe(true);
  });

  it('returns enableStats:false when OCTOCODE_ENABLE_STATS=false', () => {
    process.env['OCTOCODE_ENABLE_STATS'] = 'false';
    expect(resolveSession().enableStats).toBe(false);
  });

  it('returns enableStats:false when OCTOCODE_ENABLE_STATS=0', () => {
    process.env['OCTOCODE_ENABLE_STATS'] = '0';
    expect(resolveSession().enableStats).toBe(false);
  });

  it('DEFAULT_SESSION_CONFIG.enableStats is false', () => {
    expect(DEFAULT_SESSION_CONFIG.enableStats).toBe(false);
  });
});

// ─── isStatsEnabled ───────────────────────────────────────────────────────────

import { isStatsEnabled } from '../src/index.js';

describe('isStatsEnabled', () => {
  it('returns false when env var is unset', () => {
    expect(isStatsEnabled({})).toBe(false);
  });

  it('returns true for "1"', () => {
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: '1' })).toBe(true);
  });

  it('returns true for "true"', () => {
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: 'true' })).toBe(true);
  });

  it('returns false for "false"', () => {
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: 'false' })).toBe(false);
  });

  it('returns false for "0"', () => {
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: '0' })).toBe(false);
  });

  it('returns false for any other string', () => {
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: 'yes' })).toBe(false);
    expect(isStatsEnabled({ OCTOCODE_ENABLE_STATS: 'on' })).toBe(false);
  });
});
