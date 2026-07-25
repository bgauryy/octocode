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

  it('empty / blank OCTOCODE_HOME falls through to homedir default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('whitespace-only OCTOCODE_HOME falls through to homedir default', () => {
    const def = getOctocodeHome({ OCTOCODE_HOME: '   ' });
    expect(def.endsWith('.octocode')).toBe(true);
  });

  it('default uses os.homedir()/.octocode on every platform', async () => {
    vi.resetModules();
    vi.doMock('node:os', () => ({
      homedir: () => '/home/test',
    }));

    const { getOctocodeHome: getMockedHome } = await import('../src/home.js');
    expect(getMockedHome({})).toBe('/home/test/.octocode');
    expect(getMockedHome({ XDG_CONFIG_HOME: '/xdg', APPDATA: 'D:\\Roaming' })).toBe(
      '/home/test/.octocode',
    );

    vi.doUnmock('node:os');
    vi.resetModules();
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

  it('warns when config version is newer than this package supports', () => {
    const r = validateConfig({ version: 999 });
    expect(r.valid).toBe(true);
    expect(r.warnings).toEqual([expect.stringContaining('newer than supported')]);
  });

  it('rejects non-integer config versions', () => {
    expect(validateConfig({ version: 1.5 }).errors).toContain('version: Must be an integer');
    expect(validateConfig({ version: '1' }).errors).toContain('version: Must be an integer');
  });

  it('rejects invalid section shapes', () => {
    const r = validateConfig({
      github: [],
      local: 'nope',
      tools: [],
      network: [],
      lsp: [],
      output: [],
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(expect.arrayContaining([
      'github: Must be an object',
      'local: Must be an object',
      'tools: Must be an object',
      'network: Must be an object',
      'lsp: Must be an object',
      'output: Must be an object',
    ]));
  });

  it('rejects unsupported github URL protocols and non-string URLs', () => {
    expect(validateConfig({ github: { apiUrl: 'ftp://example.com' } }).errors).toContain(
      'github.apiUrl: Only http/https URLs allowed',
    );
    expect(validateConfig({ github: { apiUrl: 123 } }).errors).toContain(
      'github.apiUrl: Must be a string',
    );
  });

  it('rejects invalid local booleans, allowedPaths, and workspaceRoot types', () => {
    const r = validateConfig({
      local: {
        enabled: 'true',
        enableClone: 1,
        allowedPaths: ['/tmp', 42],
        workspaceRoot: 99,
      },
    });
    expect(r.errors).toEqual(expect.arrayContaining([
      'local.enabled: Must be a boolean',
      'local.enableClone: Must be a boolean',
      'local.allowedPaths[1]: Must be a string',
      'local.workspaceRoot: Must be a string',
    ]));
  });

  it('rejects relative, empty, and whitespace-only local paths', () => {
    const r = validateConfig({
      local: {
        allowedPaths: ['relative/path', '   ', '~/safe'],
        workspaceRoot: 'relative/workspace',
      },
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(expect.arrayContaining([
      expect.stringContaining('local.allowedPaths[0]: must be absolute path'),
      expect.stringContaining('local.allowedPaths[1]: empty or whitespace-only path'),
      expect.stringContaining('local.workspaceRoot: must be absolute path'),
    ]));
  });

  it('accepts null optional arrays and rejects non-array tool lists', () => {
    expect(validateConfig({ tools: { enabled: null, enableAdditional: null, disabled: null } }).valid).toBe(true);

    const r = validateConfig({
      tools: {
        enabled: 'localSearchCode',
        enableAdditional: [1],
        disabled: [false],
      },
    });
    expect(r.errors).toEqual(expect.arrayContaining([
      'tools.enabled: Must be an array',
      'tools.enableAdditional[0]: Must be a string',
      'tools.disabled[0]: Must be a string',
    ]));
  });

  it('rejects invalid network numbers and ranges', () => {
    const r = validateConfig({ network: { timeout: 'fast', maxRetries: Number.NaN } });
    expect(r.errors).toEqual(expect.arrayContaining([
      'network.timeout: Must be a number',
      'network.maxRetries: Must be a number',
    ]));

    const range = validateConfig({ network: { timeout: 1, maxRetries: 999 } });
    expect(range.errors).toEqual(expect.arrayContaining([
      expect.stringContaining('network.timeout: Must be between'),
      expect.stringContaining('network.maxRetries: Must be between'),
    ]));
  });

  it('rejects invalid lsp and output values', () => {
    const r = validateConfig({
      lsp: { configPath: 10 },
      output: {
        format: 'xml',
        pagination: { defaultCharLength: 10 },
      },
    });
    expect(r.errors).toEqual(expect.arrayContaining([
      'lsp.configPath: Must be a string',
      'output.format: Must be one of: yaml, json',
      expect.stringContaining('output.pagination.defaultCharLength: Must be between'),
    ]));

    expect(validateConfig({ output: { format: 1 } }).errors).toContain('output.format: Must be a string');
    expect(validateConfig({ output: { pagination: [] } }).errors).toContain('output.pagination: Must be an object');
    expect(validateConfig({ output: { pagination: { defaultCharLength: 'long' } } }).errors).toContain(
      'output.pagination.defaultCharLength: Must be a number',
    );
  });

  it('accepts Windows absolute local paths', () => {
    const r = validateConfig({
      local: {
        allowedPaths: ['C:\\Users\\Test'],
        workspaceRoot: 'C:\\Users\\Test',
      },
    });
    expect(r.valid).toBe(true);
  });

  it('rejects traversal path segments but allows literal dots inside a segment', () => {
    expect(validateConfig({ local: { allowedPaths: ['/tmp/project..backup'] } }).valid).toBe(true);
    const r = validateConfig({
      local: {
        allowedPaths: ['/tmp/../etc'],
        workspaceRoot: 'C:\\Users\\..\\Windows',
      },
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(expect.arrayContaining([
      expect.stringContaining('local.allowedPaths[0]'),
      expect.stringContaining('local.workspaceRoot'),
    ]));
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

  it('preserves escaped characters and comment markers inside strings', () => {
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "message": "quoted \\\" // still string /* not comment */", "keep": true }',
    );
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect((r.config as Record<string, unknown>).message).toBe('quoted " // still string /* not comment */');
  });

  it('rejects JSON values whose top-level shape is not an object', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '[]');
    const arrayResult = loadConfigSync(tmpDir);
    expect(arrayResult.success).toBe(false);
    expect(arrayResult.error).toContain('must be a JSON object');

    writeFileSync(join(tmpDir, '.octocoderc'), 'null');
    const nullResult = loadConfigSync(tmpDir);
    expect(nullResult.success).toBe(false);
    expect(nullResult.error).toContain('must be a JSON object');
  });

  it('async loadConfig delegates to sync loader', async () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ "network": { "timeout": 5000 } }');
    const { loadConfig } = await import('../src/config/loader.js');
    await expect(loadConfig(tmpDir)).resolves.toMatchObject({
      success: true,
      config: { network: { timeout: 5000 } },
    });
  });

  it('getConfigFilePath uses getOctocodeHome default when home is omitted', async () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    try {
      process.env['OCTOCODE_HOME'] = tmpDir;
      expect(getConfigFilePath()).toBe(join(tmpDir, '.octocoderc'));
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = oldHome;
    }
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
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
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
  const oldApiUrl = process.env['GITHUB_API_URL'];

  afterEach(() => {
    if (oldApiUrl === undefined) delete process.env['GITHUB_API_URL']; else process.env['GITHUB_API_URL'] = oldApiUrl;
  });

  it('uses GITHUB_API_URL env when set', () => {
    process.env['GITHUB_API_URL'] = ' https://ghe.env.example.com ';
    expect(resolveGitHub({ apiUrl: 'https://ghe.file.example.com' }).apiUrl).toBe('https://ghe.env.example.com');
  });

  it('uses fileConfig.apiUrl when no env override', () => {
    delete process.env['GITHUB_API_URL'];
    expect(resolveGitHub({ apiUrl: 'https://ghe.example.com' }).apiUrl).toBe('https://ghe.example.com');
  });
});

describe('resolveLocal', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of ['ENABLE_LOCAL', 'ENABLE_CLONE', 'ALLOWED_PATHS', 'WORKSPACE_ROOT']) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
    _resetRuntimeSurface();
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
    _resetRuntimeSurface();
  });

  it('resolves from file config and CLI default clone behavior', () => {
    setRuntimeSurface('cli');
    expect(resolveLocal().enableClone).toBe(true);
    expect(resolveLocal({ enabled: false, enableClone: false, allowedPaths: ['/tmp'], workspaceRoot: '/tmp' })).toEqual({
      enabled: false,
      enableClone: false,
      allowedPaths: ['/tmp'],
      workspaceRoot: '/tmp',
    });
  });

  it('env overrides local file config', () => {
    process.env['ENABLE_LOCAL'] = 'false';
    process.env['ENABLE_CLONE'] = 'true';
    process.env['ALLOWED_PATHS'] = ' /a, /b ,, ';
    process.env['WORKSPACE_ROOT'] = ' /workspace ';
    expect(resolveLocal({ enabled: true, enableClone: false, allowedPaths: ['/file'], workspaceRoot: '/file' })).toEqual({
      enabled: false,
      enableClone: true,
      allowedPaths: ['/a', '/b'],
      workspaceRoot: '/workspace',
    });
  });
});

describe('resolveTools', () => {
  const keys = ['TOOLS_TO_RUN', 'ENABLE_TOOLS', 'DISABLE_TOOLS'];
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of keys) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  });

  it('uses file config when env is absent and env lists when present', () => {
    expect(resolveTools({ enabled: ['a'], enableAdditional: ['b'], disabled: ['c'] })).toEqual({
      enabled: ['a'],
      enableAdditional: ['b'],
      disabled: ['c'],
    });

    process.env['TOOLS_TO_RUN'] = 'x,y';
    process.env['ENABLE_TOOLS'] = 'extra';
    process.env['DISABLE_TOOLS'] = 'blocked';
    expect(resolveTools({ enabled: ['a'], enableAdditional: ['b'], disabled: ['c'] })).toEqual({
      enabled: ['x', 'y'],
      enableAdditional: ['extra'],
      disabled: ['blocked'],
    });
  });
});

describe('resolveNetwork', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of ['REQUEST_TIMEOUT', 'MAX_RETRIES']) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  });

  it('clamps timeout to MIN/MAX bounds', () => {
    const r = resolveNetwork({ timeout: 1, maxRetries: 3 });
    expect(r.timeout).toBeGreaterThanOrEqual(MIN_TIMEOUT);
  });

  it('uses env overrides and clamps max retries', () => {
    process.env['REQUEST_TIMEOUT'] = '999999';
    process.env['MAX_RETRIES'] = '-10';
    expect(resolveNetwork({ timeout: 5000, maxRetries: 10 })).toEqual({ timeout: 300000, maxRetries: 0 });
  });
});

describe('resolveLsp', () => {
  const oldConfig = process.env['OCTOCODE_LSP_CONFIG'];
  afterEach(() => { if (oldConfig === undefined) delete process.env['OCTOCODE_LSP_CONFIG']; else process.env['OCTOCODE_LSP_CONFIG'] = oldConfig; });

  it('uses env config path before file config', () => {
    process.env['OCTOCODE_LSP_CONFIG'] = ' /env/lsp.json ';
    expect(resolveLsp({ configPath: '/file/lsp.json' }).configPath).toBe('/env/lsp.json');
  });

  it('falls back to file config when env is blank', () => {
    process.env['OCTOCODE_LSP_CONFIG'] = '   ';
    expect(resolveLsp({ configPath: '/file/lsp.json' }).configPath).toBe('/file/lsp.json');
  });
});

describe('resolveOutput', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of ['OCTOCODE_OUTPUT_FORMAT', 'OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH']) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key]; else process.env[key] = value;
    }
  });

  it('uses valid env output format and clamps default char length', () => {
    process.env['OCTOCODE_OUTPUT_FORMAT'] = ' JSON ';
    process.env['OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH'] = '999999';
    expect(resolveOutput({ format: 'yaml', pagination: { defaultCharLength: 1000 } })).toEqual({
      format: 'json',
      pagination: { defaultCharLength: 50000 },
    });
  });

  it('falls back to default format for invalid values and clamps file values', () => {
    expect(resolveOutput({ format: 'xml' as 'yaml', pagination: { defaultCharLength: 10 } })).toEqual({
      format: 'yaml',
      pagination: { defaultCharLength: 1000 },
    });
  });
});

// ─── resolverCache / getConfigSync ───────────────────────────────────────────

import { getConfig, getConfigSync, invalidateConfigCache, reloadConfig, resolveConfig, resolveConfigSync, _getCacheState, _resetConfigCache } from '../src/config/resolverCache.js';
import { getConfigValue } from '../src/config/resolver.js';

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
    expect(cfg.source).toMatch(/^(defaults|env|file|mixed|invalid)$/);
  });

  it('reports env-only overrides as env source', () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    const oldEnableLocal = process.env['ENABLE_LOCAL'];
    const home = mkdtempSync(join(tmpdir(), 'octo-source-env-'));
    try {
      process.env['OCTOCODE_HOME'] = home;
      process.env['ENABLE_LOCAL'] = 'false';
      const cfg = resolveConfigSync();
      expect(cfg.source).toBe('env');
      expect(cfg.local.enabled).toBe(false);
      expect(cfg.configPath).toBeUndefined();
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = oldHome;
      if (oldEnableLocal === undefined) delete process.env['ENABLE_LOCAL']; else process.env['ENABLE_LOCAL'] = oldEnableLocal;
    }
  });

  it('reports file source for valid .octocoderc without env overrides', () => {
    const oldEnv = { ...process.env };
    const home = mkdtempSync(join(tmpdir(), 'octo-source-file-'));
    writeFileSync(join(home, '.octocoderc'), JSON.stringify({ network: { timeout: 5000 } }));
    try {
      for (const key of [
        'GITHUB_API_URL', 'ENABLE_LOCAL', 'ENABLE_CLONE', 'ALLOWED_PATHS', 'WORKSPACE_ROOT',
        'TOOLS_TO_RUN', 'ENABLE_TOOLS', 'DISABLE_TOOLS', 'REQUEST_TIMEOUT', 'MAX_RETRIES',
        'OCTOCODE_LSP_CONFIG', 'OCTOCODE_OUTPUT_FORMAT', 'OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH',
        'OCTOCODE_ENABLE_STATS',
      ]) delete process.env[key];
      process.env['OCTOCODE_HOME'] = home;
      const cfg = resolveConfigSync();
      expect(cfg.source).toBe('file');
      expect(cfg.configPath).toBe(join(home, '.octocoderc'));
      expect(cfg.network.timeout).toBe(5000);
    } finally {
      process.env = oldEnv;
    }
  });

  it('reports mixed source for valid .octocoderc plus env overrides', () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    const oldTimeout = process.env['REQUEST_TIMEOUT'];
    const home = mkdtempSync(join(tmpdir(), 'octo-source-mixed-'));
    writeFileSync(join(home, '.octocoderc'), JSON.stringify({ network: { timeout: 5000 } }));
    try {
      process.env['OCTOCODE_HOME'] = home;
      process.env['REQUEST_TIMEOUT'] = '6000';
      const cfg = resolveConfigSync();
      expect(cfg.source).toBe('mixed');
      expect(cfg.configPath).toBe(join(home, '.octocoderc'));
      expect(cfg.network.timeout).toBe(6000);
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = oldHome;
      if (oldTimeout === undefined) delete process.env['REQUEST_TIMEOUT']; else process.env['REQUEST_TIMEOUT'] = oldTimeout;
    }
  });

  it('reports invalid semantic config as invalid without applying invalid values', () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    const home = mkdtempSync(join(tmpdir(), 'octo-source-invalid-'));
    writeFileSync(join(home, '.octocoderc'), JSON.stringify({ local: { enabled: 'nope' } }));
    try {
      process.env['OCTOCODE_HOME'] = home;
      const cfg = resolveConfigSync();
      expect(cfg.source).toBe('invalid');
      expect(cfg.configPath).toBe(join(home, '.octocoderc'));
      expect(cfg.local.enabled).toBe(true);
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = oldHome;
    }
  });

  it('reports invalid parse config as invalid', () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    const home = mkdtempSync(join(tmpdir(), 'octo-source-parse-invalid-'));
    writeFileSync(join(home, '.octocoderc'), '{bad');
    try {
      process.env['OCTOCODE_HOME'] = home;
      const cfg = resolveConfigSync();
      expect(cfg.source).toBe('invalid');
      expect(cfg.configPath).toBe(join(home, '.octocoderc'));
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME']; else process.env['OCTOCODE_HOME'] = oldHome;
    }
  });

  it('getConfigSync reflects env changes without manual invalidation', () => {
    const oldEnableLocal = process.env['ENABLE_LOCAL'];
    try {
      delete process.env['ENABLE_LOCAL'];
      const before = getConfigSync();
      process.env['ENABLE_LOCAL'] = 'false';
      const after = getConfigSync();
      expect(before.local.enabled).toBe(true);
      expect(after.local.enabled).toBe(false);
      expect(after).not.toBe(before);
      expect(_getCacheState().cached).toBe(false);
    } finally {
      if (oldEnableLocal === undefined) delete process.env['ENABLE_LOCAL']; else process.env['ENABLE_LOCAL'] = oldEnableLocal;
    }
  });

  it('session.enableStats defaults to false', () => {
    delete process.env['OCTOCODE_ENABLE_STATS'];
    const cfg = getConfigSync();
    expect(cfg.session.enableStats).toBe(false);
  });

  it('does not cache: a fresh object is returned on each call', () => {
    const a = getConfigSync();
    const b = getConfigSync();
    expect(a).not.toBe(b);
  });

  it('async resolver helpers return resolved config and do not cache', async () => {
    const resolved = await resolveConfig();
    const got = await getConfig();
    const reloaded = await reloadConfig();
    expect(resolved.github).toBeDefined();
    expect(got.github).toBeDefined();
    expect(reloaded.github).toBeDefined();
    expect(got).not.toBe(reloaded);
  });

  it('getConfigValue reads nested resolved config paths', () => {
    const oldEnableLocal = process.env['ENABLE_LOCAL'];
    try {
      process.env['ENABLE_LOCAL'] = 'false';
      expect(getConfigValue<boolean>('local.enabled')).toBe(false);
      expect(getConfigValue<string>('github.apiUrl')).toBe('https://api.github.com');
      expect(getConfigValue('local.enabled.missing')).toBeUndefined();
      expect(getConfigValue('does.not.exist')).toBeUndefined();
    } finally {
      if (oldEnableLocal === undefined) delete process.env['ENABLE_LOCAL']; else process.env['ENABLE_LOCAL'] = oldEnableLocal;
    }
  });

  it('invalidateConfigCache remains a compatibility no-op', () => {
    getConfigSync();
    invalidateConfigCache();
    expect(_getCacheState()).toEqual({ cached: false, timestamp: 0 });
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
