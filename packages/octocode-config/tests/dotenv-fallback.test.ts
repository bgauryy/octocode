import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
  HOME_TRUSTED_ENV_KEYS,
  LEGACY_ENV_NAMES,
} from '../src/config/contract.generated.js';
import { propagateOctocodeEnv } from '../src/index.js';

/** A fresh home and project (with its `.octocode`) under one temporary root. */
function makeDirs() {
  const root = mkdtempSync(join(tmpdir(), 'octocode-dotenv-'));
  const home = join(root, 'home');
  const cwd = join(root, 'project');
  mkdirSync(home);
  mkdirSync(join(cwd, '.octocode'), { recursive: true });
  return { root, home, cwd };
}

describe('trusted dotenv credential fallbacks', () => {
  let root: string;
  let home: string;
  let cwd: string;

  beforeEach(() => ({ root, home, cwd } = makeDirs()));
  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it.each(['GH_TOKEN', 'GITHUB_TOKEN'])(
    '%s uses process > trusted project > home per key',
    key => {
      writeFileSync(join(home, '.env'), `${key}=home-secret\nHOME_ONLY=home`);
      writeFileSync(join(cwd, '.octocode', '.env'), `${key}=project-secret`);
      for (const [trusted, initial, expected] of [
        [undefined, undefined, 'project-secret'],
        [false, undefined, 'home-secret'],
        [true, undefined, 'project-secret'],
        [true, 'process-secret', 'process-secret'],
      ] as const) {
        const env: Record<string, string | undefined> = { [key]: initial };
        const result = propagateOctocodeEnv({ home, cwd, trusted, env });
        expect(env[key]).toBe(expected);
        expect(env.HOME_ONLY).toBe('home');
        expect(result.sources[key]).toBe(
          trusted !== false ? 'project' : 'global'
        );
        expect(JSON.stringify(result)).not.toContain('-secret');
      }
    }
  );

  const homeOnly = new Set<string>(HOME_TRUSTED_ENV_KEYS);

  it.each([...homeOnly])(
    '%s is honored from the home .env but never from a workspace .env',
    key => {
      // A legacy name arrives under the current name it is read as.
      const current = LEGACY_ENV_NAMES[key] ?? key;
      writeFileSync(join(home, '.env'), `${key}=home-value`);
      writeFileSync(join(cwd, '.octocode', '.env'), `${key}=workspace-value`);
      for (const trusted of [undefined, true, false]) {
        const env: Record<string, string | undefined> = {};
        propagateOctocodeEnv({ home, cwd, trusted, env });
        expect(env[current]).toBe('home-value');
      }
      rmSync(join(home, '.env'));
      const env: Record<string, string | undefined> = {};
      propagateOctocodeEnv({ home, cwd, trusted: true, env });
      expect(env[current]).toBeUndefined();
    }
  );

  it.each(
    [
      ...new Set(
        CONFIG_FIELDS.flatMap(field => (field.env ? [field.env.name] : []))
      ),
    ].filter(key => !homeOnly.has(key))
  )('all product config bindings use the same source order: %s', key => {
    writeFileSync(join(home, '.env'), `${key}=home-value\nHOME_ONLY=home`);
    writeFileSync(join(cwd, '.octocode', '.env'), `${key}=workspace-value`);
    const env: Record<string, string | undefined> = {};
    propagateOctocodeEnv({ home, cwd, env });
    expect(env[key]).toBe('workspace-value');
    expect(env.HOME_ONLY).toBe('home');
    writeFileSync(join(cwd, '.octocode', '.env'), `${key}=   `);
    const fallback: Record<string, string | undefined> = {};
    propagateOctocodeEnv({ home, cwd, env: fallback });
    expect(fallback[key]).toBe('home-value');
    env[key] = 'process-value';
    propagateOctocodeEnv({ home, cwd, env });
    expect(env[key]).toBe('process-value');
  });

  it('keeps existing token alias priority after applying file fallbacks', () => {
    writeFileSync(join(home, '.env'), 'GH_TOKEN=home-secret');
    writeFileSync(join(cwd, '.octocode', '.env'), 'GH_TOKEN=project-secret');
    const env = { GITHUB_TOKEN: 'explicit-secret' };
    propagateOctocodeEnv({ home, cwd, trusted: true, env });
    // The explicit lower-priority name shadows the files' higher-priority
    // one, so the native token resolver still picks the process value.
    expect(env).toEqual({ GITHUB_TOKEN: 'explicit-secret' });
  });

  it.each([
    'GH_TOKEN',
    'TAVILY_API_KEY',
    'SERPER_API_KEY',
    'EXA_API_KEY',
    'CUSTOM_SERVICE_TOKEN',
  ])(
    '%s falls through missing and whitespace values without exposing credentials',
    key => {
      writeFileSync(join(home, '.env'), `${key}=home-secret`);
      writeFileSync(join(cwd, '.octocode', '.env'), `${key}=  `);
      for (const blank of [undefined, '', '   ']) {
        const env: Record<string, string | undefined> = { [key]: blank };
        const result = propagateOctocodeEnv({ home, cwd, env });
        expect(env[key]).toBe('home-secret');
        expect(JSON.stringify(result)).not.toContain('home-secret');
      }
    }
  );

  // The GitHub token variables are the one group that selects one value;
  // every setting has a single variable.
  for (const group of [ENV_TOKEN_VARS]) {
    for (const higherAlias of group)
      for (const lowerAlias of group) {
        it(`${higherAlias} source priority beats ${lowerAlias} alias priority`, () => {
          const pick = (env: Record<string, string | undefined>) =>
            group.map(k => env[k]?.trim()).find(Boolean);
          writeFileSync(join(home, '.env'), `${lowerAlias}=home-secret`);
          writeFileSync(
            join(cwd, '.octocode', '.env'),
            `${higherAlias}=workspace-secret`
          );
          const workspace: Record<string, string | undefined> = {};
          propagateOctocodeEnv({ home, cwd, env: workspace });
          expect(pick(workspace)).toBe('workspace-secret');
          const process: Record<string, string | undefined> = {
            [higherAlias]: 'process-secret',
          };
          writeFileSync(
            join(cwd, '.octocode', '.env'),
            `${lowerAlias}=workspace-secret`
          );
          propagateOctocodeEnv({ home, cwd, env: process });
          expect(pick(process)).toBe('process-secret');
        });
      }
  }
});

describe('classification key from dotenv files', () => {
  let root: string;
  let home: string;
  let cwd: string;

  beforeEach(() => ({ root, home, cwd } = makeDirs()));
  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it('never applies a classification key from a workspace .env', () => {
    writeFileSync(
      join(cwd, '.octocode', '.env'),
      'OCTOCODE_CLASSIFICATION_API=workspace-secret'
    );
    for (const trusted of [undefined, true]) {
      const env: Record<string, string | undefined> = {};
      const result = propagateOctocodeEnv({ home, cwd, trusted, env });
      expect(env.OCTOCODE_CLASSIFICATION_API).toBeUndefined();
      expect(result.skippedProtected).toContain('OCTOCODE_CLASSIFICATION_API');
      expect(JSON.stringify(result)).not.toContain('-secret');
    }
  });

  it('preserves the explicit classification opt-out and blocks infrastructure overrides', () => {
    writeFileSync(
      join(home, '.env'),
      'OCTOCODE_CLASSIFICATION_API=home-secret\nPATH=/bad'
    );
    writeFileSync(
      join(cwd, '.octocode', '.env'),
      'OCTOCODE_CLASSIFICATION_API=project-secret\nNODE_OPTIONS=--bad\nOCTOCODE_HOME=/bad'
    );
    const env: Record<string, string | undefined> = {
      OCTOCODE_CLASSIFICATION_API: '',
    };
    const result = propagateOctocodeEnv({ home, cwd, trusted: true, env });
    expect(env).toEqual({ OCTOCODE_CLASSIFICATION_API: '' });
    expect(result.skippedExisting).toContain('OCTOCODE_CLASSIFICATION_API');
    expect(result.skippedProtected.sort()).toEqual([
      'NODE_OPTIONS',
      'OCTOCODE_HOME',
      'PATH',
    ]);
  });
});

describe('legacy variable names', () => {
  let root: string;
  let home: string;
  let cwd: string;

  beforeEach(() => ({ root, home, cwd } = makeDirs()));
  afterEach(() => rmSync(root, { recursive: true, force: true }));

  const legacyPairs = Object.entries(LEGACY_ENV_NAMES);

  it('declares legacy names', () => {
    expect(legacyPairs.length).toBeGreaterThan(0);
  });

  it.each(legacyPairs)('%s in a .env file arrives as %s', (legacy, current) => {
    writeFileSync(join(home, '.env'), `${legacy}=home-value`);
    const env: Record<string, string | undefined> = {};
    const result = propagateOctocodeEnv({ home, cwd, env });
    expect(env[current]).toBe('home-value');
    expect(env[legacy]).toBeUndefined();
    expect(result.keys).toEqual([current]);
  });

  it.each(legacyPairs)('the current name beats %s in the same file', (legacy, current) => {
    writeFileSync(join(home, '.env'), `${legacy}=legacy-value\n${current}=current-value`);
    const env: Record<string, string | undefined> = {};
    propagateOctocodeEnv({ home, cwd, env });
    expect(env[current]).toBe('current-value');
    expect(env[legacy]).toBeUndefined();
  });

  it.each(legacyPairs)('%s in the process keeps a file from setting %s', (legacy, current) => {
    writeFileSync(join(home, '.env'), `${current}=file-value`);
    const env: Record<string, string | undefined> = { [legacy]: 'process-value' };
    const result = propagateOctocodeEnv({ home, cwd, env });
    expect(env[current]).toBeUndefined();
    expect(result.skippedExisting).toContain(current);
  });

  it.each(legacyPairs.filter(([, current]) => (ENV_TOKEN_VARS as readonly string[]).includes(current)))(
    'a workspace token shadows a home %s',
    legacy => {
      writeFileSync(join(home, '.env'), `${legacy}=home-secret`);
      for (const workspaceToken of ENV_TOKEN_VARS) {
        writeFileSync(join(cwd, '.octocode', '.env'), `${workspaceToken}=workspace-secret`);
        const env: Record<string, string | undefined> = {};
        propagateOctocodeEnv({ home, cwd, env });
        const set = Object.entries(env).filter(([, value]) => value !== undefined);
        expect(set).toEqual([[workspaceToken, 'workspace-secret']]);
      }
    }
  );
});
