/**
 * TDD verification of docs/AUTHENTICATION.md
 *
 * Each test is keyed to a specific claim in the doc. If a claim is wrong the
 * test fails; if the implementation drifts, the test catches it.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// ─── Top-level mocks (hoisted before any imports) ────────────────────────────

// Octokit mock used by §9 (cache tests via client.ts).
// A simple counter on the factory lets us distinguish instances.
const octokitMocks = vi.hoisted(() => {
  let counter = 0;
  function MockOctokit(this: Record<string, unknown>, opts: { auth?: string }) {
    this._id = ++counter;
    this._auth = opts?.auth;
  }
  MockOctokit.plugin = vi.fn().mockReturnValue(MockOctokit);
  return {
    MockOctokit,
    resetCounter: () => { counter = 0; },
  };
});

vi.mock('octokit', () => ({ Octokit: octokitMocks.MockOctokit }));
vi.mock('@octokit/plugin-throttling', () => ({ throttling: {} }));
vi.mock('../../src/session.js', () => ({ recordRateLimit: vi.fn() }));

const serverConfigMocks = vi.hoisted(() => ({
  getGitHubToken: vi.fn<[], Promise<string | null>>(),
  getServerConfig: vi.fn().mockReturnValue({
    githubApiUrl: 'https://api.github.com',
    timeout: 30000,
  }),
}));

vi.mock('../../src/serverConfig.js', async importOriginal => {
  const actual = await importOriginal<typeof import('../../src/serverConfig.js')>();
  return {
    ...actual,
    getGitHubToken: serverConfigMocks.getGitHubToken,
    getServerConfig: serverConfigMocks.getServerConfig,
  };
});

// ─── helpers ─────────────────────────────────────────────────────────────────

const ALL_ENV_VARS = [
  'OCTOCODE_TOKEN',
  'GH_TOKEN',
  'GITHUB_TOKEN',
  'GITHUB_PERSONAL_ACCESS_TOKEN',
] as const;

const saved: Partial<Record<(typeof ALL_ENV_VARS)[number], string | undefined>> = {};

function saveAndClearTokenEnv() {
  for (const v of ALL_ENV_VARS) {
    saved[v] = process.env[v];
    delete process.env[v];
  }
}

function restoreTokenEnv() {
  for (const v of ALL_ENV_VARS) {
    const prior = saved[v];
    if (prior !== undefined) process.env[v] = prior;
    else delete process.env[v];
  }
}

function makeStoredCredentials(opts: {
  expiresAt?: string;
  refreshTokenExpiresAt?: string;
}) {
  return {
    hostname: 'github.com',
    username: 'octo',
    token: {
      token: 'tok',
      tokenType: 'oauth' as const,
      ...(opts.expiresAt ? { expiresAt: opts.expiresAt } : {}),
      ...(opts.refreshTokenExpiresAt
        ? { refreshTokenExpiresAt: opts.refreshTokenExpiresAt }
        : {}),
    },
    gitProtocol: 'https' as const,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

// ─── 1. ENV_TOKEN_VARS order (docs: priority table) ──────────────────────────

describe('ENV_TOKEN_VARS priority order (AUTHENTICATION.md §Token Priority)', () => {
  it('has exactly 4 entries in documented priority order', async () => {
    vi.resetModules();
    const { ENV_TOKEN_VARS } = await import('../../src/shared/credentials/envTokens.js');
    expect([...ENV_TOKEN_VARS]).toEqual([
      'OCTOCODE_TOKEN',
      'GH_TOKEN',
      'GITHUB_TOKEN',
      'GITHUB_PERSONAL_ACCESS_TOKEN',
    ]);
  });
});

// ─── 2. resolveEnvToken priority (docs: priorities 1-4) ──────────────────────

describe('resolveEnvToken — env-var priority (AUTHENTICATION.md §Token Priority)', () => {
  beforeEach(saveAndClearTokenEnv);
  afterEach(restoreTokenEnv);

  it('OCTOCODE_TOKEN wins over all others (priority 1)', async () => {
    process.env.OCTOCODE_TOKEN = 'oc-wins';
    process.env.GH_TOKEN = 'loses';
    process.env.GITHUB_TOKEN = 'loses';
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'loses';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toMatchObject({ token: 'oc-wins', source: 'env:OCTOCODE_TOKEN' });
  });

  it('GH_TOKEN wins when OCTOCODE_TOKEN absent (priority 2)', async () => {
    process.env.GH_TOKEN = 'gh-wins';
    process.env.GITHUB_TOKEN = 'loses';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toMatchObject({ source: 'env:GH_TOKEN' });
  });

  it('GITHUB_TOKEN wins when top two absent (priority 3)', async () => {
    process.env.GITHUB_TOKEN = 'gt-wins';
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'loses';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toMatchObject({ source: 'env:GITHUB_TOKEN' });
  });

  it('GITHUB_PERSONAL_ACCESS_TOKEN used when all others absent (priority 4)', async () => {
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'pat-wins';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toMatchObject({
      token: 'pat-wins',
      source: 'env:GITHUB_PERSONAL_ACCESS_TOKEN',
    });
  });

  it('returns null when no env token is set', async () => {
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toBeNull();
  });

  it('ignores blank / whitespace-only values and falls through', async () => {
    process.env.OCTOCODE_TOKEN = '   ';
    process.env.GH_TOKEN = 'real-token';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()).toMatchObject({ source: 'env:GH_TOKEN' });
  });

  it('trims whitespace from token values', async () => {
    process.env.GITHUB_TOKEN = '  trimmed  ';
    const { resolveEnvToken } = await import('../../src/shared/credentials/envTokens.js');
    expect(resolveEnvToken()?.token).toBe('trimmed');
  });
});

// ─── 3. resolveTokenFull full chain (docs: priorities 5 & 6) ─────────────────

describe('resolveTokenFull — storage + gh-cli fallback (AUTHENTICATION.md §Token Priority)', () => {
  beforeEach(() => {
    saveAndClearTokenEnv();
    vi.resetModules();
  });
  afterEach(restoreTokenEnv);

  it('env token wins over storage (priority 1-4 > 5)', async () => {
    process.env.GH_TOKEN = 'env-wins';
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: 'stored', source: 'stored' }),
    });
    const r = await mod.resolveTokenFull({ getGhCliToken: () => null });
    expect(r?.source).toBe('env:GH_TOKEN');
  });

  it('falls back to storage when env absent, source is "octocode-storage"', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({
        token: 'stored-tok',
        source: 'stored',
        username: 'octo',
      }),
    });
    const r = await mod.resolveTokenFull({ getGhCliToken: () => null });
    expect(r).toMatchObject({ token: 'stored-tok', source: 'octocode-storage', wasRefreshed: false });
  });

  it('wasRefreshed=true when storage auto-refreshed the token', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: 'new-tok', source: 'refreshed' }),
    });
    expect((await mod.resolveTokenFull({ getGhCliToken: () => null }))?.wasRefreshed).toBe(true);
  });

  it('falls back to gh-cli when storage is empty, source is "gh-cli"', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: null, source: 'none' }),
    });
    const r = await mod.resolveTokenFull({ getGhCliToken: () => 'gh-token' });
    expect(r).toMatchObject({ token: 'gh-token', source: 'gh-cli' });
  });

  it('gh-cli token is trimmed before use', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: null, source: 'none' }),
    });
    expect((await mod.resolveTokenFull({ getGhCliToken: () => '  spaced  ' }))?.token).toBe('spaced');
  });

  it('passes hostname to gh-cli getter (Enterprise support)', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    const ghGetter = vi.fn().mockReturnValue(null);
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: null, source: 'none' }),
    });
    await mod.resolveTokenFull({ hostname: 'enterprise.example.com', getGhCliToken: ghGetter });
    expect(ghGetter).toHaveBeenCalledWith('enterprise.example.com');
  });

  it('gh-cli errors are swallowed and return null (not thrown)', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: null, source: 'none' }),
    });
    const r = await mod.resolveTokenFull({ getGhCliToken: () => { throw new Error('gh not found'); } });
    expect(r).toBeNull();
  });

  it('returns null when all sources exhausted', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    mod.initTokenResolution({
      getTokenWithRefresh: vi.fn().mockResolvedValue({ token: null, source: 'none' }),
    });
    expect(await mod.resolveTokenFull({ getGhCliToken: () => null })).toBeNull();
  });

  it('throws when called before initTokenResolution', async () => {
    const mod = await import('../../src/shared/credentials/tokenResolution.js');
    await expect(mod.resolveTokenFull()).rejects.toThrow('Token resolution not initialized');
  });
});

// ─── 4. Source label strings (docs §Token Priority — "Source labels") ─────────

describe('TokenSource label strings match documented values (AUTHENTICATION.md §Token Priority)', () => {
  beforeEach(saveAndClearTokenEnv);
  afterEach(restoreTokenEnv);

  it.each([
    ['OCTOCODE_TOKEN', 'env:OCTOCODE_TOKEN'],
    ['GH_TOKEN', 'env:GH_TOKEN'],
    ['GITHUB_TOKEN', 'env:GITHUB_TOKEN'],
    ['GITHUB_PERSONAL_ACCESS_TOKEN', 'env:GITHUB_PERSONAL_ACCESS_TOKEN'],
  ] as const)('%s → source "%s"', async (envVar, expectedSource) => {
    process.env[envVar] = 'test-token';
    const { getEnvTokenSource } = await import('../../src/shared/credentials/envTokens.js');
    expect(getEnvTokenSource()).toBe(expectedSource);
  });
});

// ─── 5. Encryption (docs: AES-256-GCM, format iv:authTag:ciphertext) ──────────

describe('Credential encryption format (AUTHENTICATION.md §Encryption)', () => {
  it('encrypt/decrypt roundtrip is lossless', async () => {
    const { encrypt, decrypt } = await import(
      '../../src/shared/credentials/credentialEncryption.js'
    );
    const plain = JSON.stringify({ hostname: 'github.com', username: 'octo' });
    expect(decrypt(encrypt(plain))).toBe(plain);
  });

  it('encrypted format is three colon-separated hex parts (iv:authTag:ciphertext)', async () => {
    const { encrypt } = await import(
      '../../src/shared/credentials/credentialEncryption.js'
    );
    const parts = encrypt('test data').split(':');
    expect(parts).toHaveLength(3);
    parts.forEach(p => expect(p).toMatch(/^[0-9a-f]+$/i));
  });

  it('uses a fresh random IV each call — two encryptions differ', async () => {
    const { encrypt } = await import(
      '../../src/shared/credentials/credentialEncryption.js'
    );
    const plain = 'same-plaintext';
    expect(encrypt(plain)).not.toBe(encrypt(plain));
  });

  it('IV portion has the expected length (16 bytes = 32 hex chars)', async () => {
    const { encrypt } = await import(
      '../../src/shared/credentials/credentialEncryption.js'
    );
    const ivHex = encrypt('anything').split(':')[0];
    expect(ivHex).toHaveLength(32);
  });
});

// ─── 6. Storage file paths (docs §Credential Storage) ────────────────────────

describe('Storage file paths (AUTHENTICATION.md §Credential Storage)', () => {
  it('credentials file is <home>/credentials.json', async () => {
    const { paths } = await import('../../src/shared/paths.js');
    expect(paths.credentials).toContain('credentials.json');
    expect(paths.credentials).toContain(paths.home);
  });

  it('key file is <home>/.key', async () => {
    const { paths } = await import('../../src/shared/paths.js');
    expect(paths.key).toContain('.key');
    expect(paths.key).toContain(paths.home);
  });

  it('CREDENTIALS_FILE and KEY_FILE match paths.*', async () => {
    const enc = await import('../../src/shared/credentials/credentialEncryption.js');
    const { paths } = await import('../../src/shared/paths.js');
    expect(enc.CREDENTIALS_FILE).toBe(paths.credentials);
    expect(enc.KEY_FILE).toBe(paths.key);
  });
});

// ─── 7. Home directory (docs §Credential Storage table) ──────────────────────

describe('Default home directory name (AUTHENTICATION.md §Credential Storage)', () => {
  it('home directory contains ".octocode"', async () => {
    const { OCTOCODE_HOME } = await import('../../src/shared/paths.js');
    expect(OCTOCODE_HOME).toContain('.octocode');
  });
});

// ─── 8. Token expiry guard (docs: "within 5 minutes of now") ─────────────────

describe('Token expiry 5-minute guard (AUTHENTICATION.md §Token Refresh)', () => {
  it('no expiresAt → never expired (OAuth App tokens)', async () => {
    const { isTokenExpired } = await import('../../src/shared/credentials/credentialUtils.js');
    expect(isTokenExpired(makeStoredCredentials({}))).toBe(false);
  });

  it('expires in 10 minutes → NOT expired yet', async () => {
    const { isTokenExpired } = await import('../../src/shared/credentials/credentialUtils.js');
    const expiresAt = new Date(Date.now() + 10 * 60 * 1000).toISOString();
    expect(isTokenExpired(makeStoredCredentials({ expiresAt }))).toBe(false);
  });

  it('expires in 3 minutes → IS expired (inside 5-minute buffer)', async () => {
    const { isTokenExpired } = await import('../../src/shared/credentials/credentialUtils.js');
    const expiresAt = new Date(Date.now() + 3 * 60 * 1000).toISOString();
    expect(isTokenExpired(makeStoredCredentials({ expiresAt }))).toBe(true);
  });

  it('already past expiry → expired', async () => {
    const { isTokenExpired } = await import('../../src/shared/credentials/credentialUtils.js');
    const expiresAt = new Date(Date.now() - 1000).toISOString();
    expect(isTokenExpired(makeStoredCredentials({ expiresAt }))).toBe(true);
  });

  it('invalid expiresAt date string → treated as expired', async () => {
    const { isTokenExpired } = await import('../../src/shared/credentials/credentialUtils.js');
    expect(isTokenExpired(makeStoredCredentials({ expiresAt: 'not-a-date' }))).toBe(true);
  });
});

// ─── 9. Refresh token expiry (docs §Token Refresh) ───────────────────────────

describe('Refresh token expiry (AUTHENTICATION.md §Token Refresh)', () => {
  it('no refreshTokenExpiresAt → not expired', async () => {
    const { isRefreshTokenExpired } = await import(
      '../../src/shared/credentials/credentialUtils.js'
    );
    expect(isRefreshTokenExpired(makeStoredCredentials({}))).toBe(false);
  });

  it('future refreshTokenExpiresAt → not expired', async () => {
    const { isRefreshTokenExpired } = await import(
      '../../src/shared/credentials/credentialUtils.js'
    );
    const refreshTokenExpiresAt = new Date(Date.now() + 60 * 60 * 1000).toISOString();
    expect(isRefreshTokenExpired(makeStoredCredentials({ refreshTokenExpiresAt }))).toBe(false);
  });

  it('past refreshTokenExpiresAt → expired, must re-login', async () => {
    const { isRefreshTokenExpired } = await import(
      '../../src/shared/credentials/credentialUtils.js'
    );
    const refreshTokenExpiresAt = new Date(Date.now() - 1000).toISOString();
    expect(isRefreshTokenExpired(makeStoredCredentials({ refreshTokenExpiresAt }))).toBe(true);
  });
});

// ─── 10. Octokit cache — same token → same instance (docs §Octokit Instance Cache)

describe('Octokit instance cache (AUTHENTICATION.md §Octokit Instance Cache)', () => {
  beforeEach(() => {
    octokitMocks.resetCounter();
    serverConfigMocks.getGitHubToken.mockReset();
    serverConfigMocks.getServerConfig.mockReturnValue({
      githubApiUrl: 'https://api.github.com',
      timeout: 30000,
    });
  });

  it('same token within TTL → same cached instance', async () => {
    vi.resetModules();
    serverConfigMocks.getGitHubToken.mockResolvedValue('stable-token');
    const { getOctokit, clearOctokitInstances } = await import('../../src/github/client.js');
    clearOctokitInstances();

    const first = await getOctokit();
    const second = await getOctokit();
    expect(first).toBe(second);
  });

  it('authInfo.token overrides resolved token → different instances for different tokens', async () => {
    vi.resetModules();
    serverConfigMocks.getGitHubToken.mockResolvedValue('resolved-token');
    const { getOctokit, clearOctokitInstances } = await import('../../src/github/client.js');
    clearOctokitInstances();

    const withExplicit = await getOctokit({ token: 'explicit-token' });
    const withResolved = await getOctokit();
    // Two different tokens → two different instances
    expect(withExplicit).not.toBe(withResolved);
  });

  it('resolves token fresh on every call (no startup static cache)', async () => {
    vi.resetModules();
    let callCount = 0;
    serverConfigMocks.getGitHubToken.mockImplementation(() =>
      Promise.resolve(++callCount <= 999 ? 'always-fresh' : 'always-fresh')
    );
    const { getOctokit, clearOctokitInstances } = await import('../../src/github/client.js');
    clearOctokitInstances();

    await getOctokit();
    await getOctokit();

    expect(serverConfigMocks.getGitHubToken).toHaveBeenCalledTimes(2);
  });
});

// ─── 11. Hostname normalisation (docs §In-Memory Credential Cache) ────────────

describe('Hostname normalisation (AUTHENTICATION.md §In-Memory Credential Cache)', () => {
  it.each([
    ['https://GitHub.com/', 'github.com'],
    ['http://github.com', 'github.com'],
    ['GITHUB.COM', 'github.com'],
    ['github.com', 'github.com'],
    ['enterprise.example.com', 'enterprise.example.com'],
  ])('normalizeHostname(%s) → %s', async (input, expected) => {
    const { normalizeHostname } = await import(
      '../../src/shared/credentials/credentialUtils.js'
    );
    expect(normalizeHostname(input)).toBe(expected);
  });

  it('"https://GitHub.com/" and "github.com" normalise to the same key', async () => {
    const { normalizeHostname } = await import(
      '../../src/shared/credentials/credentialUtils.js'
    );
    expect(normalizeHostname('https://GitHub.com/')).toBe(normalizeHostname('github.com'));
  });
});

// ─── 12. TokenSourceType valid set (docs §Token Priority) ────────────────────

describe('VALID_TOKEN_SOURCES covers all documented source strings (AUTHENTICATION.md §Token Priority)', () => {
  const DOCUMENTED_SOURCES = [
    'env:OCTOCODE_TOKEN',
    'env:GH_TOKEN',
    'env:GITHUB_TOKEN',
    'env:GITHUB_PERSONAL_ACCESS_TOKEN',
    'octocode-storage',
    'gh-cli',
  ] as const;

  it.each(DOCUMENTED_SOURCES)(
    'source "%s" passes through serverConfig.getTokenSource unchanged',
    async src => {
      vi.resetModules();
      // Import the real serverConfig and override _resolveTokenFull
      const sc = await import('../../src/serverConfig.js');
      sc._setTokenResolvers({
        resolveTokenFull: vi.fn().mockResolvedValue({ token: 'tok', source: src }),
      });
      const result = await sc.getTokenSource();
      expect(result).toBe(src);
      sc._resetTokenResolvers();
    }
  );

  it('"none" is returned when resolveTokenFull returns null', async () => {
    vi.resetModules();
    const sc = await import('../../src/serverConfig.js');
    sc._setTokenResolvers({
      resolveTokenFull: vi.fn().mockResolvedValue(null),
    });
    expect(await sc.getTokenSource()).toBe('none');
    sc._resetTokenResolvers();
  });

  it('unrecognised source string is coerced to "none"', async () => {
    vi.resetModules();
    const sc = await import('../../src/serverConfig.js');
    sc._setTokenResolvers({
      resolveTokenFull: vi.fn().mockResolvedValue({ token: 'tok', source: 'bogus-source' }),
    });
    expect(await sc.getTokenSource()).toBe('none');
    sc._resetTokenResolvers();
  });
});
