import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  ENV_TOKEN_VARS,
  getTokenFromEnv,
  getEnvTokenSource,
  hasEnvToken,
} from '../../../src/shared/credentials/envTokens.js';

const SAVED_ENV: Record<string, string | undefined> = {};

function saveAndClear() {
  for (const v of ENV_TOKEN_VARS) {
    SAVED_ENV[v] = process.env[v];
    delete process.env[v];
  }
}

function restore() {
  for (const v of ENV_TOKEN_VARS) {
    if (SAVED_ENV[v] !== undefined) {
      process.env[v] = SAVED_ENV[v];
    } else {
      delete process.env[v];
    }
  }
}

describe('ENV_TOKEN_VARS', () => {
  it('should have OCTOCODE_TOKEN first (highest priority)', () => {
    expect(ENV_TOKEN_VARS[0]).toBe('OCTOCODE_TOKEN');
  });

  it('should have GH_TOKEN second', () => {
    expect(ENV_TOKEN_VARS[1]).toBe('GH_TOKEN');
  });

  it('should have GITHUB_TOKEN third', () => {
    expect(ENV_TOKEN_VARS[2]).toBe('GITHUB_TOKEN');
  });

  it('should have GITHUB_PERSONAL_ACCESS_TOKEN fourth (lowest priority)', () => {
    expect(ENV_TOKEN_VARS[3]).toBe('GITHUB_PERSONAL_ACCESS_TOKEN');
  });

  it('should have exactly 4 entries', () => {
    expect(ENV_TOKEN_VARS).toHaveLength(4);
  });
});

describe('getTokenFromEnv', () => {
  beforeEach(saveAndClear);
  afterEach(restore);

  it('returns null when no env vars set', () => {
    expect(getTokenFromEnv()).toBeNull();
  });

  it('returns OCTOCODE_TOKEN when set', () => {
    process.env.OCTOCODE_TOKEN = 'octo-token-123';
    expect(getTokenFromEnv()).toBe('octo-token-123');
  });

  it('returns GH_TOKEN when OCTOCODE_TOKEN is absent', () => {
    process.env.GH_TOKEN = 'gh-token-456';
    expect(getTokenFromEnv()).toBe('gh-token-456');
  });

  it('returns GITHUB_TOKEN when both higher-priority vars absent', () => {
    process.env.GITHUB_TOKEN = 'github-token-789';
    expect(getTokenFromEnv()).toBe('github-token-789');
  });

  it('prefers OCTOCODE_TOKEN over GH_TOKEN and GITHUB_TOKEN', () => {
    process.env.OCTOCODE_TOKEN = 'winner';
    process.env.GH_TOKEN = 'loser1';
    process.env.GITHUB_TOKEN = 'loser2';
    expect(getTokenFromEnv()).toBe('winner');
  });

  it('prefers GH_TOKEN over GITHUB_TOKEN', () => {
    process.env.GH_TOKEN = 'winner';
    process.env.GITHUB_TOKEN = 'loser';
    expect(getTokenFromEnv()).toBe('winner');
  });

  it('returns GITHUB_PERSONAL_ACCESS_TOKEN when all higher-priority vars absent', () => {
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'pat-token-abc';
    expect(getTokenFromEnv()).toBe('pat-token-abc');
  });

  it('prefers GITHUB_TOKEN over GITHUB_PERSONAL_ACCESS_TOKEN', () => {
    process.env.GITHUB_TOKEN = 'winner';
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'loser';
    expect(getTokenFromEnv()).toBe('winner');
  });

  it('prefers OCTOCODE_TOKEN over GITHUB_PERSONAL_ACCESS_TOKEN', () => {
    process.env.OCTOCODE_TOKEN = 'winner';
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'loser';
    expect(getTokenFromEnv()).toBe('winner');
  });

  it('trims whitespace', () => {
    process.env.GITHUB_TOKEN = '  trimmed  ';
    expect(getTokenFromEnv()).toBe('trimmed');
  });

  it('ignores empty string', () => {
    process.env.OCTOCODE_TOKEN = '';
    expect(getTokenFromEnv()).toBeNull();
  });

  it('ignores whitespace-only string', () => {
    process.env.GH_TOKEN = '   ';
    expect(getTokenFromEnv()).toBeNull();
  });
});

describe('getEnvTokenSource', () => {
  beforeEach(saveAndClear);
  afterEach(restore);

  it('returns null when no env vars set', () => {
    expect(getEnvTokenSource()).toBeNull();
  });

  it('returns "env:OCTOCODE_TOKEN" when OCTOCODE_TOKEN set', () => {
    process.env.OCTOCODE_TOKEN = 'tok';
    expect(getEnvTokenSource()).toBe('env:OCTOCODE_TOKEN');
  });

  it('returns "env:GH_TOKEN" when GH_TOKEN set', () => {
    process.env.GH_TOKEN = 'tok';
    expect(getEnvTokenSource()).toBe('env:GH_TOKEN');
  });

  it('returns "env:GITHUB_TOKEN" when GITHUB_TOKEN set', () => {
    process.env.GITHUB_TOKEN = 'tok';
    expect(getEnvTokenSource()).toBe('env:GITHUB_TOKEN');
  });

  it('follows priority: OCTOCODE_TOKEN > GH_TOKEN > GITHUB_TOKEN', () => {
    process.env.OCTOCODE_TOKEN = 'a';
    process.env.GH_TOKEN = 'b';
    process.env.GITHUB_TOKEN = 'c';
    expect(getEnvTokenSource()).toBe('env:OCTOCODE_TOKEN');
  });

  it('returns "env:GITHUB_PERSONAL_ACCESS_TOKEN" when only PAT is set', () => {
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'tok';
    expect(getEnvTokenSource()).toBe('env:GITHUB_PERSONAL_ACCESS_TOKEN');
  });

  it('GITHUB_TOKEN wins over GITHUB_PERSONAL_ACCESS_TOKEN', () => {
    process.env.GITHUB_TOKEN = 'a';
    process.env.GITHUB_PERSONAL_ACCESS_TOKEN = 'b';
    expect(getEnvTokenSource()).toBe('env:GITHUB_TOKEN');
  });
});

describe('hasEnvToken', () => {
  beforeEach(saveAndClear);
  afterEach(restore);

  it('returns false when no env vars set', () => {
    expect(hasEnvToken()).toBe(false);
  });

  it('returns true when any token env var is set', () => {
    process.env.GITHUB_TOKEN = 'tok';
    expect(hasEnvToken()).toBe(true);
  });
});
