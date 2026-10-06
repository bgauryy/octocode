import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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
    // Keep the developer's own `gh` login out of the token checks.
    vi.stubEnv('GH_CONFIG_DIR', path.join(home, 'gh'));
    for (const key of allKeys) vi.stubEnv(key, '');
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    rmSync(home, { recursive: true, force: true });
  });

  it('lists every GitHub token name and the classification key', () => {
    const keys = SKILL_ENV_PARAMS['octocode-rfc-generator']!.map(p => p.key);
    expect(keys).toEqual(allKeys);
    expect(classificationKeys).toEqual(['OCTOCODE_CLASSIFICATION_API']);
    expect(groupLabel('github-token')).toContain(ENV_TOKEN_VARS.join(', '));
  });

  it('accepts each GitHub token name', () => {
    for (const name of ENV_TOKEN_VARS) {
      for (const key of ENV_TOKEN_VARS) vi.stubEnv(key, '');
      expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
      vi.stubEnv(name, 'token');
      expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
    }
  });

  it('counts a stored `octocode auth login` or `gh` login as the GitHub token', () => {
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
    writeFileSync(path.join(home, 'credentials.json'), '{}');
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
    rmSync(path.join(home, 'credentials.json'));
    mkdirSync(path.join(home, 'gh'), { recursive: true });
    writeFileSync(path.join(home, 'gh', 'hosts.yml'), 'github.com: {}\n');
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
  });

  it('honors the home .env layer Octocode loads', () => {
    const key = ENV_TOKEN_VARS.at(-1)!;
    expect(isEnvSet(key)).toBe(false);
    writeFileSync(path.join(home, '.env'), `${key}=from-home\n`);
    expect(isEnvSet(key)).toBe(true);
  });

  it('honors the workspace .env layer, as the native resolver does', () => {
    const workspace = mkdtempSync(path.join(tmpdir(), 'octocode-env-ws-'));
    mkdirSync(path.join(workspace, '.octocode'));
    vi.spyOn(process, 'cwd').mockReturnValue(workspace);
    const token = ENV_TOKEN_VARS[0]!;
    expect(isEnvSet(token)).toBe(false);
    writeFileSync(
      path.join(workspace, '.octocode', '.env'),
      `${token}=from-repo\n`
    );
    expect(isEnvSet(token)).toBe(true);
    rmSync(workspace, { recursive: true, force: true });
  });
});
