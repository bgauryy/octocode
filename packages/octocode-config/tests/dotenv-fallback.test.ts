import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import {
  CONFIG_FIELDS,
  ENV_TOKEN_VARS,
} from '../src/config/contract.generated.js';
import { propagateOctocodeEnv, resolveEnvToken } from '../src/index.js';

describe('trusted dotenv credential fallbacks', () => {
  let root: string;
  let home: string;
  let cwd: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'octocode-dotenv-'));
    home = join(root, 'home');
    cwd = join(root, 'project');
    mkdirSync(home);
    mkdirSync(join(cwd, '.octocode'), { recursive: true });
  });
  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it.each([
    'OCTOCODE_TOKEN',
    'GH_TOKEN',
    'GITHUB_TOKEN',
    'GITHUB_PERSONAL_ACCESS_TOKEN',
    'OCTOCODE_CLASSIFICATION_API',
    'OCTOCODE_JEV_KEY',
    'OCTOCODE_CLASSIFICATION_TYPE',
    'OCTOCODE_CLASSIFICATION_API_HOST',
  ])('%s uses process > trusted project > home per key', key => {
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
  });

  it.each([
    ...new Set(
      CONFIG_FIELDS.flatMap(field => field.env.map(binding => binding.name))
    ),
  ])('all product config bindings use the same source order: %s', key => {
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
    const env = { OCTOCODE_TOKEN: 'explicit-secret' };
    propagateOctocodeEnv({ home, cwd, trusted: true, env });
    expect(resolveEnvToken(env)?.token).toBe('explicit-secret');
  });

  it.each([
    'GH_TOKEN',
    'OCTOCODE_JEV_KEY',
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

  const tokenGroups: readonly (readonly string[])[] = [
    ENV_TOKEN_VARS,
    ...CONFIG_FIELDS.filter(f => f.credential && f.env.length > 1).map(f =>
      f.env.map(e => e.name)
    ),
  ];
  for (const group of tokenGroups) {
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
