import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { beforeEach, describe, expect, it } from 'vitest';

import {
  applyOctocodeEnv,
  DEFAULT_CONFIG,
  ENV_TOKEN_VARS,
  loadOctocodeEnv,
  propagateOctocodeEnv,
} from '../src/index.js';
import { isProtectedKey, PROTECTED_KEYS } from '../src/dotenv.js';
import { CONFIG_FIELDS } from '../src/config/contract.generated.js';

describe('PROTECTED_KEYS', () => {
  it('matches protected keys case-insensitively only on Windows', () => {
    const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
    try {
      Object.defineProperty(process, 'platform', { value: 'darwin' });
      expect(isProtectedKey('Path')).toBe(false);
      Object.defineProperty(process, 'platform', { value: 'win32' });
      expect(isProtectedKey('Path')).toBe(true);
      expect(isProtectedKey('TAVILY_API_KEY')).toBe(false);
    } finally {
      Object.defineProperty(process, 'platform', platform);
    }
  });

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

  it('allows both auth token vars as trusted file fallbacks', () => {
    for (const k of ['GH_TOKEN', 'GITHUB_TOKEN']) {
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
      'OCTOCODE_LSP_AUTO_INSTALL',
      'OCTOCODE_LSP_CACHE_DIR',
    ]) {
      expect(PROTECTED_KEYS.has(k), `${k} should be protected`).toBe(true);
    }
    expect(PROTECTED_KEYS.has('OCTOCODE_LSP_PREWARM')).toBe(false);
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
      // A trusted-project .env must not relocate a child's config home.
      'OCTOCODE_HOME',
      // Home-only: a workspace .env must not redirect credentials, widen the
      // sandbox, or pick or download executables.
      'OCTOCODE_LSP_AUTO_INSTALL',
      'OCTOCODE_LSP_CACHE_DIR',
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
      // The OAuth app a login or refresh uses: a repository must not choose it.
      'OCTOCODE_GITHUB_CLIENT_ID',
      // A workspace may narrow, never widen: local tools off, redaction on.
      'OCTOCODE_ENABLE_LOCAL',
      'OCTOCODE_REDACT_EMAILS',
    ];
    expect([...PROTECTED_KEYS].sort()).toEqual([...CANONICAL].sort());
  });
});

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

describe('loadOctocodeEnv', () => {
  it('skips blank values in the home .env', () => {
    const home = mkdtempSync(join(tmpdir(), 'octo-blank-'));
    writeFileSync(join(home, '.env'), 'EMPTY=\nSET=v\n');
    expect(loadOctocodeEnv({ home }).map).toEqual({ SET: 'v' });
  });

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

  it('never reads the home .env a second time as the workspace layer', () => {
    // cwd = the OS home makes <cwd>/.octocode the Octocode home.
    const osHome = join(tmpDir, 'user');
    mkdirSync(join(osHome, '.octocode'), { recursive: true });
    writeFileSync(join(osHome, '.octocode', '.env'), 'GH_TOKEN=home-token\n');
    const { map, sources } = loadOctocodeEnv({
      home: join(osHome, '.octocode'),
      cwd: osHome,
    });
    expect(map.GH_TOKEN).toBe('home-token');
    expect(sources.GH_TOKEN).toBe('global');
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

describe('ENV_TOKEN_VARS', () => {
  it('lists both token vars in priority order', () => {
    expect(ENV_TOKEN_VARS).toEqual(['GH_TOKEN', 'GITHUB_TOKEN']);
  });
});

/** The value at a dotted settings path. */
function valueAt(config: unknown, path: string): unknown {
  let node = config;
  for (const key of path.split('.'))
    node = typeof node === 'object' && node !== null ? Reflect.get(node, key) : undefined;
  return node;
}

describe('DEFAULT_CONFIG', () => {
  it('holds the default each declared setting names', () => {
    for (const field of CONFIG_FIELDS)
      expect(valueAt(DEFAULT_CONFIG, field.path) ?? null, field.path).toEqual(
        field.defaultValue ?? null
      );
  });

  it('an optional path setting defaults to unset', () => {
    expect(DEFAULT_CONFIG.lsp.cacheDir).toBeUndefined();
    const field = CONFIG_FIELDS.find(field => field.path === 'lsp.cacheDir')!;
    expect(field).toEqual(expect.objectContaining({ type: 'path', defaultValue: null }));
  });
});
