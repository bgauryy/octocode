import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { configFieldEnvNames, ENV_TOKEN_VARS } from '@octocodeai/config';
import {
  getSkillEnvStatus,
  groupLabel,
  isEnvSet,
  SKILL_ENV_PARAMS,
} from '../../../../src/cli/commands/skills/env-params.js';

const classificationKeys = configFieldEnvNames('classification.api');
const allKeys = [...ENV_TOKEN_VARS, ...classificationKeys];

describe('skill env params come from the config contract', () => {
  let home: string;

  beforeEach(() => {
    home = mkdtempSync(path.join(tmpdir(), 'octocode-env-params-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    for (const key of allKeys) vi.stubEnv(key, '');
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    rmSync(home, { recursive: true, force: true });
  });

  it('lists every GitHub token name and classification key alias', () => {
    const keys = SKILL_ENV_PARAMS['octocode-rfc-generator']!.map(p => p.key);
    expect(keys).toEqual(allKeys);
    expect(classificationKeys.length).toBeGreaterThan(1);
    expect(groupLabel('github-token')).toContain(ENV_TOKEN_VARS.join(', '));
  });

  it('accepts any token alias, including OCTOCODE_TOKEN', () => {
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
    vi.stubEnv(ENV_TOKEN_VARS[0], 'token');
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
  });

  it('honors the home .env layer Octocode loads', () => {
    const key = ENV_TOKEN_VARS.at(-1)!;
    expect(isEnvSet(key)).toBe(false);
    writeFileSync(path.join(home, '.env'), `${key}=from-home\n`);
    expect(isEnvSet(key)).toBe(true);
  });
});
