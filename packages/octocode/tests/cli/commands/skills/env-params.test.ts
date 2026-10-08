import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { configFieldEnvNames, ENV_TOKEN_VARS } from '@octocodeai/config';
import {
  envParamRows,
  getSkillEnvStatus,
  groupLabel,
  isEnvSet,
  SKILL_ENV_PARAMS,
} from '../../../../src/cli/commands/skills/env-params.js';

const classificationKeys = configFieldEnvNames('classification.api');
const allKeys = [...ENV_TOKEN_VARS, ...classificationKeys];

/**
 * A stand-in native binary whose `config check <token> --json` reports
 * `source` as the GitHub token it would use (none when undefined).
 */
function fakeNative(dir: string, source?: string): string {
  const bin = path.join(dir, `octocode-${source ?? 'none'}`);
  const reply = JSON.stringify({
    key: ENV_TOKEN_VARS[0],
    set: false,
    ...(source ? { githubTokenSource: source } : {}),
  });
  const expected = JSON.stringify([
    'config',
    'check',
    ENV_TOKEN_VARS[0],
    '--json',
  ]);
  writeFileSync(
    bin,
    `#!/usr/bin/env node\nif (JSON.stringify(process.argv.slice(2)) === ${JSON.stringify(expected)}) process.stdout.write(${JSON.stringify(reply)});\nprocess.exit(1);\n`
  );
  chmodSync(bin, 0o755);
  return bin;
}

describe('skill env params come from the config contract', () => {
  let home: string;

  beforeEach(() => {
    home = mkdtempSync(path.join(tmpdir(), 'octocode-env-params-'));
    vi.stubEnv('OCTOCODE_HOME', home);
    // Keep the developer's own stored or `gh` login out of the token checks.
    vi.stubEnv('OCTOCODE_NATIVE_BIN', fakeNative(home));
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

  it('keeps keyless research paths usable and redacts optional provider values', () => {
    vi.stubEnv('SCRAPING_ANT', '');
    for (const key of ['TAVILY_API_KEY', 'EXA_API_KEY', 'SERPER_API_KEY'])
      vi.stubEnv(key, '');
    const brainstorming = getSkillEnvStatus('octocode-brainstorming');
    expect(brainstorming.readiness).toBe('ready');
    expect(envParamRows(brainstorming)).toEqual(
      ['TAVILY_API_KEY', 'EXA_API_KEY', 'SERPER_API_KEY'].map(key =>
        expect.objectContaining({
          key,
          status: 'missing',
          required: 'optional',
        })
      )
    );
    expect(getSkillEnvStatus('octocode-scraping').readiness).toBe('ready');
    vi.stubEnv('SCRAPING_ANT', 'test-provider-secret');
    const rows = envParamRows(getSkillEnvStatus('octocode-scraping'));
    expect(rows).toEqual([
      expect.objectContaining({
        key: 'SCRAPING_ANT',
        status: 'set',
        required: 'optional',
      }),
    ]);
    expect(JSON.stringify(rows)).not.toContain('test-provider-secret');
  });

  it('accepts each GitHub token name', () => {
    for (const name of ENV_TOKEN_VARS) {
      for (const key of ENV_TOKEN_VARS) vi.stubEnv(key, '');
      expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
      vi.stubEnv(name, 'token');
      expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
    }
  });

  it('asks native for a GitHub token outside the environment', () => {
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
    vi.stubEnv('OCTOCODE_NATIVE_BIN', fakeNative(home, 'octocode-storage'));
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
    vi.stubEnv('OCTOCODE_NATIVE_BIN', fakeNative(home, 'gh-cli'));
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('ready');
  });

  it('does not read credential files itself', () => {
    writeFileSync(path.join(home, 'credentials.json'), '{}');
    mkdirSync(path.join(home, 'gh'), { recursive: true });
    writeFileSync(path.join(home, 'gh', 'hosts.yml'), 'github.com: {}\n');
    vi.stubEnv('GH_CONFIG_DIR', path.join(home, 'gh'));
    expect(getSkillEnvStatus('octocode-research').readiness).toBe('partial');
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
