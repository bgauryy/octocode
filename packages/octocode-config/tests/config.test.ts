import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  applyOctocodeEnv,
  getOctocodeHome,
  isPersistentStorageEnabled,
  isStatsEnabled,
  loadOctocodeEnv,
  loadOctocoderc,
  parseEnv,
  PROTECTED_KEYS,
  propagateOctocodeEnv,
} from '../src/index.js';

describe('storage policy', () => {
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  const previousStats = process.env['OCTOCODE_ENABLE_STATS'];
  const previousHome = process.env['OCTOCODE_HOME'];
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'octo-storage-'));
    process.env['OCTOCODE_HOME'] = tmpDir;
  });

  afterEach(() => {
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    if (previousStats === undefined)
      delete process.env['OCTOCODE_ENABLE_STATS'];
    else process.env['OCTOCODE_ENABLE_STATS'] = previousStats;
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = previousHome;
  });

  it('disables all disk-backed runtime state in memory mode', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
    process.env['OCTOCODE_ENABLE_STATS'] = 'true';
    expect(isPersistentStorageEnabled()).toBe(false);
    expect(isStatsEnabled()).toBe(false);
  });

  it('defaults to persistent when no env var and no rc file', () => {
    delete process.env['OCTOCODE_STORAGE_MODE'];
    expect(isPersistentStorageEnabled()).toBe(true);
  });

  it('reads storage.mode from .octocoderc when env var is absent', () => {
    delete process.env['OCTOCODE_STORAGE_MODE'];
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      JSON.stringify({ storage: { mode: 'memory' } })
    );
    expect(isPersistentStorageEnabled()).toBe(false);

    writeFileSync(
      join(tmpDir, '.octocoderc'),
      JSON.stringify({ storage: { mode: 'persistent' } })
    );
    expect(isPersistentStorageEnabled()).toBe(true);
  });

  it('honors the home .env and lets a workspace only narrow to memory', () => {
    delete process.env['OCTOCODE_STORAGE_MODE'];
    const workspace = mkdtempSync(join(tmpdir(), 'octo-storage-ws-'));
    mkdirSync(join(workspace, '.octocode'));
    writeFileSync(join(tmpDir, '.env'), 'OCTOCODE_STORAGE_MODE=memory\n');
    expect(isPersistentStorageEnabled(process.env, workspace)).toBe(false);

    writeFileSync(join(tmpDir, '.env'), '');
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      JSON.stringify({ storage: { mode: 'memory' } })
    );
    writeFileSync(
      join(workspace, '.octocode', '.env'),
      'OCTOCODE_STORAGE_MODE=persistent\n'
    );
    writeFileSync(
      join(workspace, '.octocode', '.octocoderc'),
      JSON.stringify({ storage: { mode: 'persistent' } })
    );
    expect(isPersistentStorageEnabled(process.env, workspace)).toBe(false);

    writeFileSync(join(tmpDir, '.octocoderc'), JSON.stringify({}));
    writeFileSync(
      join(workspace, '.octocode', '.octocoderc'),
      JSON.stringify({ storage: { mode: 'memory' } })
    );
    expect(isPersistentStorageEnabled(process.env, workspace)).toBe(false);
  });

  it('env var wins over .octocoderc storage.mode', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      JSON.stringify({ storage: { mode: 'persistent' } })
    );
    expect(isPersistentStorageEnabled()).toBe(false);
  });
});

// ─── getOctocodeHome ─────────────────────────────────────────────────────────

describe('getOctocodeHome', () => {
  it('OCTOCODE_HOME override wins, path is resolved', () => {
    expect(getOctocodeHome({ OCTOCODE_HOME: '/custom/home' })).toBe(
      '/custom/home'
    );
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
    expect(
      getMockedHome({ XDG_CONFIG_HOME: '/xdg', APPDATA: 'D:\\Roaming' })
    ).toBe('/home/test/.octocode');

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
    for (const k of [
      'PATH',
      'HOME',
      'SHELL',
      'USER',
      'LOGNAME',
      'PWD',
      'TMPDIR',
      'NODE_OPTIONS',
      'PYTHON',
    ]) {
      expect(PROTECTED_KEYS.has(k), `${k} should be protected`).toBe(true);
    }
  });

  it('allows all four auth token vars as trusted file fallbacks', () => {
    for (const k of [
      'OCTOCODE_TOKEN',
      'GH_TOKEN',
      'GITHUB_TOKEN',
      'GITHUB_PERSONAL_ACCESS_TOKEN',
    ]) {
      expect(PROTECTED_KEYS.has(k)).toBe(false);
    }
  });

  it('keeps endpoints, sandbox roots, and executables out of a workspace .env', () => {
    for (const k of [
      'GITHUB_API_URL',
      'OCTOCODE_CLASSIFICATION_API_HOST',
      'OCTOCODE_ALLOW_PRIVATE_REGISTRY',
      'OCTOCODE_LSP_CONFIG',
      'ALLOWED_PATHS',
      'WORKSPACE_ROOT',
      'OCTOCODE_BETA',
      'OCTOCODE_CARGO',
      'OCTOCODE_TS_SERVER_PATH',
      'GH_HOST',
    ]) {
      expect(PROTECTED_KEYS.has(k), `${k} should be protected`).toBe(true);
    }
    expect(PROTECTED_KEYS.has('REQUEST_TIMEOUT')).toBe(false);
  });

  it('does not protect tool API keys (they go in .env)', () => {
    expect(PROTECTED_KEYS.has('TAVILY_API_KEY')).toBe(false);
    expect(PROTECTED_KEYS.has('SERPER_API_KEY')).toBe(false);
  });

  // Parity with the native runtime's canonical set
  // (crates/runtime/src/config/types.rs `PROTECTED_KEYS`). The two lists must
  // stay identical so a `.env` cannot bypass a protection on one side only;
  // update both together when adding a key.
  it('equals the native (Rust) PROTECTED_KEYS set exactly', () => {
    const CANONICAL = [
      'PATH',
      'HOME',
      'SHELL',
      'USER',
      'LOGNAME',
      'PWD',
      'TMPDIR',
      'NODE_OPTIONS',
      'PYTHON',
      'GH_HOST',
      // A trusted-project .env must not relocate a child's config home.
      'OCTOCODE_HOME',
      // Home-only: a workspace .env must not redirect credentials, widen the
      // sandbox, or pick executables.
      'OCTOCODE_TS_SERVER_PATH',
      'OCTOCODE_RUST_SERVER_PATH',
      'OCTOCODE_GO_SERVER_PATH',
      'OCTOCODE_PYTHON_SERVER_PATH',
      'OCTOCODE_JAVA_SERVER_PATH',
      'OCTOCODE_CLANGD_SERVER_PATH',
      'OCTOCODE_CSHARP_SERVER_PATH',
      'OCTOCODE_SCALA_SERVER_PATH',
      'OCTOCODE_ASM_SERVER_PATH',
      'OCTOCODE_TRUST_PROJECT_LSP_CONFIG',
      // Storage mode decides what persists on disk: home-trusted only.
      'OCTOCODE_STORAGE_MODE',
      'OCTOCODE_CARGO',
      'GITHUB_API_URL',
      'OCTOCODE_BETA',
      'ALLOWED_PATHS',
      'WORKSPACE_ROOT',
      'OCTOCODE_ALLOW_PRIVATE_REGISTRY',
      'OCTOCODE_LSP_CONFIG',
      'OCTOCODE_CLASSIFICATION_API_HOST',
    ];
    expect([...PROTECTED_KEYS].sort()).toEqual([...CANONICAL].sort());
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
    expect(parseEnv('URL=https://example.com?a=1&b=2').URL).toBe(
      'https://example.com?a=1&b=2'
    );
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
      { PATH: '/evil', NODE_OPTIONS: '--bad', OCTOCODE_HOME: '/evil' },
      { env }
    );
    expect(env).toEqual({});
    expect(res.skippedProtected).toEqual([
      'PATH',
      'NODE_OPTIONS',
      'OCTOCODE_HOME',
    ]);
  });

  it('lets a workspace .env only opt out of persistence, never in', () => {
    const widen: Record<string, string | undefined> = {};
    const skipped = applyOctocodeEnv(
      { OCTOCODE_STORAGE_MODE: 'persistent' },
      { env: widen, sources: { OCTOCODE_STORAGE_MODE: 'project' } }
    );
    expect(widen).toEqual({});
    expect(skipped.skippedProtected).toEqual(['OCTOCODE_STORAGE_MODE']);

    const narrow: Record<string, string | undefined> = {};
    const applied = applyOctocodeEnv(
      { OCTOCODE_STORAGE_MODE: ' Memory ' },
      { env: narrow, sources: { OCTOCODE_STORAGE_MODE: 'project' } }
    );
    expect(narrow.OCTOCODE_STORAGE_MODE).toBe(' Memory ');
    expect(applied.skippedProtected).toEqual([]);

    const home: Record<string, string | undefined> = {};
    applyOctocodeEnv(
      { OCTOCODE_STORAGE_MODE: 'persistent' },
      { env: home, sources: { OCTOCODE_STORAGE_MODE: 'global' } }
    );
    expect(home.OCTOCODE_STORAGE_MODE).toBe('persistent');
  });

  it('skips already-set (non-empty) keys and reports them', () => {
    const env: Record<string, string | undefined> = { EXISTING: 'keep' };
    const res = applyOctocodeEnv({ EXISTING: 'new' }, { env });
    expect(env.EXISTING).toBe('keep');
    expect(res.skippedExisting).toContain('EXISTING');
  });

  it('keeps a blank OCTOCODE_CLASSIFICATION_API as an explicit opt-out', () => {
    const env: Record<string, string | undefined> = {
      OCTOCODE_CLASSIFICATION_API: '',
    };
    const res = applyOctocodeEnv(
      { OCTOCODE_CLASSIFICATION_API: 'from-home' },
      { env, sources: { OCTOCODE_CLASSIFICATION_API: 'global' } }
    );
    expect(env.OCTOCODE_CLASSIFICATION_API).toBe('');
    expect(res.skippedExisting).toContain('OCTOCODE_CLASSIFICATION_API');
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

  it('workspace .env can narrow but never widen a global storage mode', () => {
    writeFileSync(join(home, '.env'), 'OCTOCODE_STORAGE_MODE=persistent\n');
    writeFileSync(
      join(cwd, '.octocode', '.env'),
      'OCTOCODE_STORAGE_MODE=memory\n'
    );
    const narrowed = loadOctocodeEnv({ home, cwd });
    expect(narrowed.map.OCTOCODE_STORAGE_MODE).toBe('memory');
    expect(narrowed.sources.OCTOCODE_STORAGE_MODE).toBe('project');

    writeFileSync(join(home, '.env'), 'OCTOCODE_STORAGE_MODE=memory\n');
    writeFileSync(
      join(cwd, '.octocode', '.env'),
      'OCTOCODE_STORAGE_MODE=persistent\n'
    );
    const kept = loadOctocodeEnv({ home, cwd });
    expect(kept.map.OCTOCODE_STORAGE_MODE).toBe('memory');
    expect(kept.sources.OCTOCODE_STORAGE_MODE).toBe('global');
  });

  it('project .env NOT loaded when trusted=false', () => {
    writeFileSync(join(cwd, '.octocode', '.env'), 'PROJECT_KEY=project\n');
    const { map } = loadOctocodeEnv({ home, cwd, trusted: false });
    expect('PROJECT_KEY' in map).toBe(false);
  });

  it('project .env loaded and overrides global when trusted=true', () => {
    writeFileSync(join(home, '.env'), 'SHARED=global\nGLOBAL_ONLY=g\n');
    writeFileSync(
      join(cwd, '.octocode', '.env'),
      'SHARED=project\nPROJECT_ONLY=p\n'
    );

    const { map, sources } = loadOctocodeEnv({ home, cwd, trusted: true });
    expect(map.SHARED).toBe('project');
    expect(map.GLOBAL_ONLY).toBe('g');
    expect(map.PROJECT_ONLY).toBe('p');
    expect(sources.PROJECT_ONLY).toBe('project');
    expect(sources.GLOBAL_ONLY).toBe('global');
  });

  it('returns empty map when home is missing', () => {
    const { map } = loadOctocodeEnv({
      home: '/does/not/exist',
      cwd: undefined,
    });
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
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "network": { "timeout": 5000 } }'
    );
    expect(loadOctocoderc(tmpDir)).toEqual({ network: { timeout: 5000 } });
  });

  it('strips line comments', () => {
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{\n  // comment\n  "key": "val"\n}'
    );
    expect(loadOctocoderc(tmpDir)).toEqual({ key: 'val' });
  });

  it('strips block comments', () => {
    writeFileSync(join(tmpDir, '.octocoderc'), '{ /* block */ "key": "val" }');
    expect(loadOctocoderc(tmpDir)).toEqual({ key: 'val' });
  });

  it('tolerates trailing commas', () => {
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "network": { "timeout": 1234, }, }'
    );
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
      '{ "github": { "apiUrl": "https://api.github.com" } }'
    );
    const rc = loadOctocoderc(tmpDir);
    expect((rc.github as Record<string, string>).apiUrl).toBe(
      'https://api.github.com'
    );
  });

  it('writes parse error to stderr, does not throw', () => {
    const spy = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    writeFileSync(join(tmpDir, '.octocoderc'), 'BAD JSON');
    const result = loadOctocoderc(tmpDir);
    expect(result).toEqual({});
    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('[octocode-config]')
    );
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
    expect(getTokenFromEnv({ GITHUB_PERSONAL_ACCESS_TOKEN: 'tok4' })).toBe(
      'tok4'
    );
  });

  it('OCTOCODE_TOKEN beats GH_TOKEN', () => {
    expect(getTokenFromEnv({ OCTOCODE_TOKEN: 'high', GH_TOKEN: 'low' })).toBe(
      'high'
    );
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
    expect(getEnvTokenSource({ OCTOCODE_TOKEN: 'x' })).toBe(
      'env:OCTOCODE_TOKEN'
    );
    expect(getEnvTokenSource({ GH_TOKEN: 'x' })).toBe('env:GH_TOKEN');
    expect(getEnvTokenSource({ GITHUB_PERSONAL_ACCESS_TOKEN: 'x' })).toBe(
      'env:GITHUB_PERSONAL_ACCESS_TOKEN'
    );
  });
});

describe('hasEnvToken', () => {
  it('false when no token', () => expect(hasEnvToken({})).toBe(false));
  it('true when any token set', () =>
    expect(hasEnvToken({ GH_TOKEN: 'x' })).toBe(true));
});

describe('resolveEnvToken', () => {
  it('returns null when no token', () =>
    expect(resolveEnvToken({})).toBeNull());
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
    expect(DEFAULT_CONFIG.local.beta).toBe(false);
    expect(DEFAULT_NETWORK_CONFIG.timeout).toBe(30000);
    expect(DEFAULT_NETWORK_CONFIG.allowPrivateRegistry).toBe(false);
    expect(DEFAULT_CONFIG.output.redactEmails).toBe(false);
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

  it('accepts only supported storage modes', () => {
    expect(validateConfig({ storage: { mode: 'memory' } }).valid).toBe(true);
    const invalid = validateConfig({ storage: { mode: 'disk' } });
    expect(invalid.valid).toBe(false);
    expect(invalid.errors).toContain(
      'storage.mode: Must be "persistent" or "memory"'
    );

    const invalidShape = validateConfig({ storage: 'memory' });
    expect(invalidShape.valid).toBe(false);
    expect(invalidShape.errors).toContain('storage: Must be an object');
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

  it('warns on unknown nested keys instead of silently ignoring typos', () => {
    const r = validateConfig({
      github: { apiUrl: 'https://api.github.com', apiURL: 'typo' },
      local: { enabled: true, enableLocl: false },
      tools: { enabled: null, enableAdditonal: ['artifactSearch'] },
      network: { timeout: 30000, retries: 2 },
      lsp: { configPath: '/tmp/lsp.json', config: 'typo' },
      classification: { type: 'jev', typ: 'typo' },
      output: {
        format: 'yaml',
        formatter: 'typo',
        pagination: { defaultCharLength: 20000, defaultChars: 10 },
      },
    });

    expect(r.valid).toBe(true);
    expect(r.warnings).toEqual(
      expect.arrayContaining([
        'Unknown configuration key: github.apiURL',
        'Unknown configuration key: local.enableLocl',
        'Unknown configuration key: tools.enableAdditonal',
        'Unknown configuration key: network.retries',
        'Unknown configuration key: lsp.config',
        'Unknown configuration key: classification.typ',
        'Unknown configuration key: output.formatter',
        'Unknown configuration key: output.pagination.defaultChars',
      ])
    );
  });

  it('warns when config version is newer than this package supports', () => {
    const r = validateConfig({ version: 999 });
    expect(r.valid).toBe(true);
    expect(r.warnings).toEqual([
      expect.stringContaining('newer than supported'),
    ]);
  });

  it('rejects non-integer config versions', () => {
    expect(validateConfig({ version: 1.5 }).errors).toContain(
      'version: Must be an integer'
    );
    expect(validateConfig({ version: '1' }).errors).toContain(
      'version: Must be an integer'
    );
  });

  it('rejects invalid section shapes', () => {
    const r = validateConfig({
      github: [],
      local: 'nope',
      tools: [],
      network: [],
      lsp: [],
      classification: [],
      output: [],
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(
      expect.arrayContaining([
        'github: Must be an object',
        'local: Must be an object',
        'tools: Must be an object',
        'network: Must be an object',
        'lsp: Must be an object',
        'classification: Must be an object',
        'output: Must be an object',
      ])
    );
  });

  it('rejects unsupported github URL protocols and non-string URLs', () => {
    expect(
      validateConfig({ github: { apiUrl: 'ftp://example.com' } }).errors
    ).toContain('github.apiUrl: Only http/https URLs allowed');
    expect(validateConfig({ github: { apiUrl: 123 } }).errors).toContain(
      'github.apiUrl: Must be a string'
    );
  });

  it('rejects invalid local booleans, allowedPaths, and workspaceRoot types', () => {
    const r = validateConfig({
      local: {
        enabled: 'true',
        beta: 'yes',
        allowedPaths: ['/tmp', 42],
        workspaceRoot: 99,
      },
    });
    expect(r.errors).toEqual(
      expect.arrayContaining([
        'local.enabled: Must be a boolean',
        'local.beta: Must be a boolean',
        'local.allowedPaths[1]: Must be a string',
        'local.workspaceRoot: Must be a string',
      ])
    );
  });

  it('rejects relative, empty, and whitespace-only local paths', () => {
    const r = validateConfig({
      local: {
        allowedPaths: ['relative/path', '   ', '~/safe'],
        workspaceRoot: 'relative/workspace',
      },
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(
      expect.arrayContaining([
        expect.stringContaining('local.allowedPaths[0]: must be absolute path'),
        expect.stringContaining(
          'local.allowedPaths[1]: empty or whitespace-only path'
        ),
        expect.stringContaining('local.workspaceRoot: must be absolute path'),
      ])
    );
  });

  it('accepts null optional arrays and rejects non-array tool lists', () => {
    expect(
      validateConfig({ tools: { enabled: null, disabled: null } }).valid
    ).toBe(true);

    const r = validateConfig({
      tools: {
        enabled: 'localSearch',
        disabled: [false],
      },
    });
    expect(r.errors).toEqual(
      expect.arrayContaining([
        'tools.enabled: Must be an array',
        'tools.disabled[0]: Must be a string',
      ])
    );
  });

  it('rejects invalid network numbers and ranges', () => {
    const r = validateConfig({
      network: { timeout: 'fast', maxRetries: Number.NaN },
    });
    expect(r.errors).toEqual(
      expect.arrayContaining([
        'network.timeout: Must be a number',
        'network.maxRetries: Must be a number',
      ])
    );

    const range = validateConfig({ network: { timeout: 1, maxRetries: 999 } });
    expect(range.errors).toEqual(
      expect.arrayContaining([
        expect.stringContaining('network.timeout: Must be between'),
        expect.stringContaining('network.maxRetries: Must be between'),
      ])
    );

    expect(
      validateConfig({ network: { allowPrivateRegistry: 'yes' } }).errors
    ).toContain('network.allowPrivateRegistry: Must be a boolean');
  });

  it('rejects invalid lsp and output values', () => {
    const r = validateConfig({
      lsp: { configPath: 10 },
      output: {
        format: 'xml',
        pagination: { defaultCharLength: 10 },
      },
    });
    expect(r.errors).toEqual(
      expect.arrayContaining([
        'lsp.configPath: Must be a string',
        'output.format: Must be one of: yaml, json',
        expect.stringContaining(
          'output.pagination.defaultCharLength: Must be between'
        ),
      ])
    );

    expect(validateConfig({ output: { format: 1 } }).errors).toContain(
      'output.format: Must be a string'
    );
    expect(validateConfig({ output: { pagination: [] } }).errors).toContain(
      'output.pagination: Must be an object'
    );
    expect(
      validateConfig({ output: { pagination: { defaultCharLength: 'long' } } })
        .errors
    ).toContain('output.pagination.defaultCharLength: Must be a number');
    expect(
      validateConfig({ output: { redactEmails: 'yes' } }).errors
    ).toContain('output.redactEmails: Must be a boolean');
  });

  it('validates classification credential fallback values and URL protocols', () => {
    expect(
      validateConfig({
        classification: {
          type: 'jev',
          api: 'secret',
          apiHost: 'https://api.example.test/v1',
        },
      }).valid
    ).toBe(true);

    const invalidTypes = validateConfig({
      classification: { api: 1, apiHost: 2 },
    });
    expect(invalidTypes.errors).toEqual(
      expect.arrayContaining([
        'classification.api: Must be a string',
        'classification.apiHost: Must be a string',
      ])
    );

    expect(
      validateConfig({ classification: { apiHost: 'file:///tmp/provider' } })
        .errors
    ).toContain('classification.apiHost: Only http/https URLs allowed');
    expect(
      validateConfig({ classification: { apiHost: 'not a URL' } }).errors
    ).toContain('classification.apiHost: Invalid URL format');
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
    expect(
      validateConfig({ local: { allowedPaths: ['/tmp/project..backup'] } })
        .valid
    ).toBe(true);
    const r = validateConfig({
      local: {
        allowedPaths: ['/tmp/../etc'],
        workspaceRoot: 'C:\\Users\\..\\Windows',
      },
    });
    expect(r.valid).toBe(false);
    expect(r.errors).toEqual(
      expect.arrayContaining([
        expect.stringContaining('local.allowedPaths[0]'),
        expect.stringContaining('local.workspaceRoot'),
      ])
    );
  });
});

// ─── loadConfigSync (via loader) ─────────────────────────────────────────────

import {
  loadConfigSync,
  configExists,
  getConfigFilePath,
} from '../src/config/loader.js';

describe('loadConfigSync', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'octo-loader-'));
  });

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
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "github": { "apiUrl": "https://api.github.com" } }'
    );
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect(
      (r.config as Record<string, Record<string, string>>).github?.apiUrl
    ).toBe('https://api.github.com');
  });

  it('preserves escaped characters and comment markers inside strings', () => {
    writeFileSync(
      join(tmpDir, '.octocoderc'),
      '{ "message": "quoted \\\" // still string /* not comment */", "keep": true }'
    );
    const r = loadConfigSync(tmpDir);
    expect(r.success).toBe(true);
    expect((r.config as Record<string, unknown>).message).toBe(
      'quoted " // still string /* not comment */'
    );
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

  it('getConfigFilePath uses getOctocodeHome default when home is omitted', async () => {
    const oldHome = process.env['OCTOCODE_HOME'];
    try {
      process.env['OCTOCODE_HOME'] = tmpDir;
      expect(getConfigFilePath()).toBe(join(tmpDir, '.octocoderc'));
    } finally {
      if (oldHome === undefined) delete process.env['OCTOCODE_HOME'];
      else process.env['OCTOCODE_HOME'] = oldHome;
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
  resolveStorage,
} from '../src/config/resolverSections.js';

describe('parseBooleanEnv', () => {
  it.each([
    ['true', true],
    ['1', true],
    ['false', false],
    ['0', false],
  ])('parses "%s" → %s', (input, expected) =>
    expect(parseBooleanEnv(input)).toBe(expected)
  );
  it('returns undefined for blank / unknown', () => {
    expect(parseBooleanEnv(undefined)).toBeUndefined();
    expect(parseBooleanEnv('')).toBeUndefined();
    expect(parseBooleanEnv('yes')).toBeUndefined();
  });
});

describe('parseIntEnv', () => {
  it('parses integer strings', () => expect(parseIntEnv('42')).toBe(42));
  it('returns undefined for non-numeric', () =>
    expect(parseIntEnv('abc')).toBeUndefined());
  it('returns undefined for undefined', () =>
    expect(parseIntEnv(undefined)).toBeUndefined());
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
    if (oldApiUrl === undefined) delete process.env['GITHUB_API_URL'];
    else process.env['GITHUB_API_URL'] = oldApiUrl;
  });

  it('uses GITHUB_API_URL env when set', () => {
    process.env['GITHUB_API_URL'] = ' https://ghe.env.example.com ';
    expect(
      resolveGitHub({ apiUrl: 'https://ghe.file.example.com' }).apiUrl
    ).toBe('https://ghe.env.example.com');
  });

  it('uses fileConfig.apiUrl when no env override', () => {
    delete process.env['GITHUB_API_URL'];
    expect(resolveGitHub({ apiUrl: 'https://ghe.example.com' }).apiUrl).toBe(
      'https://ghe.example.com'
    );
  });
});

describe('resolveLocal', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of [
      'ENABLE_LOCAL',
      'OCTOCODE_BETA',
      'ALLOWED_PATHS',
      'WORKSPACE_ROOT',
    ]) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
    _resetRuntimeSurface();
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
    _resetRuntimeSurface();
  });

  it('defaults local.enabled on for every runtime surface', () => {
    setRuntimeSurface('cli');
    expect(resolveLocal().enabled).toBe(true);
    setRuntimeSurface('mcp');
    expect(resolveLocal().enabled).toBe(true);
  });

  it('the removed local.enableClone key is an unknown-key warning, not an error', () => {
    const r = validateConfig({ local: { enableClone: true } });
    expect(r.errors).toEqual([]);
    expect(r.warnings).toContain(
      'Unknown configuration key: local.enableClone'
    );
    expect('enableClone' in resolveLocal()).toBe(false);
  });

  it('resolves explicit local file config', () => {
    expect(
      resolveLocal({
        enabled: false,
        beta: false,
        allowedPaths: ['/tmp'],
        workspaceRoot: '/tmp',
      })
    ).toEqual({
      enabled: false,
      beta: false,
      allowedPaths: ['/tmp'],
      workspaceRoot: '/tmp',
    });
  });

  it('env overrides local file config', () => {
    process.env['ENABLE_LOCAL'] = 'false';
    process.env['OCTOCODE_BETA'] = 'true';
    process.env['ALLOWED_PATHS'] = ' /a, /b ,, ';
    process.env['WORKSPACE_ROOT'] = ' /workspace ';
    expect(
      resolveLocal({
        enabled: true,
        beta: false,
        allowedPaths: ['/file'],
        workspaceRoot: '/file',
      })
    ).toEqual({
      enabled: false,
      beta: true,
      allowedPaths: ['/a', '/b'],
      workspaceRoot: '/workspace',
    });
  });
});

describe('resolveTools', () => {
  const keys = ['TOOLS_TO_RUN', 'DISABLE_TOOLS'];
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of keys) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it('uses file config when env is absent and env lists when present', () => {
    expect(resolveTools({ enabled: ['a'], disabled: ['c'] })).toEqual({
      enabled: ['a'],
      disabled: ['c'],
      family: 'all',
    });

    process.env['TOOLS_TO_RUN'] = 'x,y';
    process.env['DISABLE_TOOLS'] = 'blocked';
    expect(resolveTools({ enabled: ['a'], disabled: ['c'] })).toEqual({
      enabled: ['x', 'y'],
      disabled: ['blocked'],
      family: 'all',
    });
  });
});

describe('resolveStorage', () => {
  const previous = process.env['OCTOCODE_STORAGE_MODE'];

  afterEach(() => {
    if (previous === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previous;
  });

  it('defaults to persistent storage and accepts a file opt-out', () => {
    delete process.env['OCTOCODE_STORAGE_MODE'];
    expect(resolveStorage()).toEqual({ mode: 'persistent' });
    expect(resolveStorage({ mode: 'memory' })).toEqual({ mode: 'memory' });
  });

  it('lets the environment force memory-only operation', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
    expect(resolveStorage({ mode: 'persistent' })).toEqual({ mode: 'memory' });
  });

  it('does not let an invalid environment value defeat a file privacy choice', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'memroy';
    expect(resolveStorage({ mode: 'memory' })).toEqual({ mode: 'memory' });
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
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it('clamps timeout to MIN/MAX bounds', () => {
    const r = resolveNetwork({ timeout: 1, maxRetries: 3 });
    expect(r.timeout).toBeGreaterThanOrEqual(MIN_TIMEOUT);
  });

  it('uses env overrides and clamps max retries', () => {
    process.env['REQUEST_TIMEOUT'] = '999999';
    process.env['MAX_RETRIES'] = '-10';
    expect(resolveNetwork({ timeout: 5000, maxRetries: 10 })).toEqual({
      timeout: 300000,
      maxRetries: 0,
      allowPrivateRegistry: false,
    });
  });

  it('uses the private-registry env opt-in before file config', () => {
    process.env['OCTOCODE_ALLOW_PRIVATE_REGISTRY'] = 'true';
    expect(
      resolveNetwork({ allowPrivateRegistry: false }).allowPrivateRegistry
    ).toBe(true);
  });
});

describe('resolveLsp', () => {
  const oldConfig = process.env['OCTOCODE_LSP_CONFIG'];
  afterEach(() => {
    if (oldConfig === undefined) delete process.env['OCTOCODE_LSP_CONFIG'];
    else process.env['OCTOCODE_LSP_CONFIG'] = oldConfig;
  });

  it('uses env config path before file config', () => {
    process.env['OCTOCODE_LSP_CONFIG'] = ' /env/lsp.json ';
    expect(resolveLsp({ configPath: '/file/lsp.json' }).configPath).toBe(
      '/env/lsp.json'
    );
  });

  it('falls back to file config when env is blank', () => {
    process.env['OCTOCODE_LSP_CONFIG'] = '   ';
    expect(resolveLsp({ configPath: '/file/lsp.json' }).configPath).toBe(
      '/file/lsp.json'
    );
  });
});

describe('resolveOutput', () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of [
      'OCTOCODE_OUTPUT_FORMAT',
      'OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH',
      'OCTOCODE_REDACT_EMAILS',
    ]) {
      savedEnv[key] = process.env[key];
      delete process.env[key];
    }
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it('uses valid env output format and clamps default char length', () => {
    process.env['OCTOCODE_OUTPUT_FORMAT'] = ' JSON ';
    process.env['OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH'] = '999999';
    expect(
      resolveOutput({ format: 'yaml', pagination: { defaultCharLength: 1000 } })
    ).toEqual({
      format: 'json',
      pagination: { defaultCharLength: 50000 },
      redactEmails: false,
    });
  });

  it('falls back to default format for invalid values and clamps file values', () => {
    expect(
      resolveOutput({
        format: 'xml' as 'yaml',
        pagination: { defaultCharLength: 10 },
      })
    ).toEqual({
      format: 'yaml',
      pagination: { defaultCharLength: 1000 },
      redactEmails: false,
    });
  });

  it('uses the email-redaction env opt-in before file config', () => {
    process.env['OCTOCODE_REDACT_EMAILS'] = 'true';
    expect(resolveOutput({ redactEmails: false }).redactEmails).toBe(true);
  });
});

// ─── resolveSession ───────────────────────────────────────────────────────────

import { resolveSession } from '../src/config/resolverSections.js';
import { DEFAULT_SESSION_CONFIG } from '../src/config/defaults.js';

describe('resolveSession', () => {
  afterEach(() => {
    delete process.env['OCTOCODE_ENABLE_STATS'];
  });

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

describe('isStatsEnabled', () => {
  it.each(['TRUE', ' true ', ' 1 '])(
    'uses the shared Boolean parser for %s',
    value => {
      expect(
        isStatsEnabled({
          OCTOCODE_ENABLE_STATS: value,
          OCTOCODE_STORAGE_MODE: 'persistent',
        })
      ).toBe(true);
    }
  );

  it('normalizes the memory storage gate before allowing stats writes', () => {
    expect(
      isStatsEnabled({
        OCTOCODE_ENABLE_STATS: 'true',
        OCTOCODE_STORAGE_MODE: ' MEMORY ',
      })
    ).toBe(false);
  });

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

// ─── loader non-Error throw ──────────────────────────────────────────────────

describe('loadConfigSync non-Error throw', () => {
  it('stringifies non-Error values thrown while reading', async () => {
    vi.resetModules();
    vi.doMock('node:fs', () => ({
      existsSync: () => true,
      readFileSync: () => {
        throw 'raw-string-failure';
      },
    }));

    const { loadConfigSync: mockedLoad } =
      await import('../src/config/loader.js');
    const r = mockedLoad('/nonexistent-home');
    expect(r.success).toBe(false);
    expect(r.error).toBe('Failed to parse config file: raw-string-failure');

    vi.doUnmock('node:fs');
    vi.resetModules();
  });
});
