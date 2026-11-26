import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  CONFIG_FIELDS,
  canonicalEnv,
  canonicalEnvName,
  configFieldEnvName,
  getKey,
  loadOctocodeEnv,
} from '../src/index.js';

/** Every alias the contract declares, with the current name it reads as. */
const ALIASES = CONFIG_FIELDS.flatMap(field =>
  (field.env?.aliases ?? []).map(alias => [alias, field.env!.name] as const)
);
const CURRENT = new Set(
  CONFIG_FIELDS.flatMap(field => (field.env ? [field.env.name] : []))
);
const dirs: string[] = [];

function tempDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-aliases-'));
  dirs.push(dir);
  return dir;
}

afterEach(() => {
  for (const dir of dirs.splice(0))
    fs.rmSync(dir, { recursive: true, force: true });
});

describe('environment variable aliases', () => {
  it('reads each alias as one current name, and a current name as itself', () => {
    expect(ALIASES.length).toBeGreaterThan(0);
    for (const [alias, name] of ALIASES) {
      expect(canonicalEnvName(alias), alias).toBe(name);
      expect(canonicalEnvName(name), name).toBe(name);
      expect(CURRENT.has(alias), `${alias} is also a current name`).toBe(false);
      expect(
        ALIASES.filter(([other]) => other === alias),
        alias
      ).toHaveLength(1);
    }
    expect(canonicalEnvName('PATH')).toBe('PATH');
  });

  it('moves a value to the current name, which wins when both are set', () => {
    for (const [alias, name] of ALIASES) {
      expect(canonicalEnv({ [alias]: 'old' }), alias).toEqual({
        [name]: 'old',
      });
      expect(canonicalEnv({ [alias]: 'old', [name]: 'new' }), alias).toEqual({
        [name]: 'new',
      });
      expect(canonicalEnv({ [name]: 'new', [alias]: 'old' }), alias).toEqual({
        [name]: 'new',
      });
    }
    expect(canonicalEnv({ PATH: '/bin', UNSET: undefined })).toEqual({
      PATH: '/bin',
    });
  });

  // The dotenv policy is keyed by current names, so a file is read through
  // the alias rule first: a home-only switch stays home-only under its
  // earlier name.
  it('applies the dotenv policy to an alias as to its current name', () => {
    const name = configFieldEnvName('tools.enableSecurity');
    const alias = ALIASES.find(([, current]) => current === name)?.[0];
    expect(name).toBeDefined();
    expect(alias).toBeDefined();
    const home = tempDir();
    const cwd = tempDir();
    fs.mkdirSync(path.join(cwd, '.octocode'));
    fs.writeFileSync(path.join(home, '.env'), `${alias}=false\n`);
    fs.writeFileSync(path.join(cwd, '.octocode', '.env'), `${alias}=true\n`);
    const { map, sources } = loadOctocodeEnv({ home, cwd });
    expect(map).toEqual({ [name as string]: 'false' });
    expect(sources).toEqual({ [name as string]: 'global' });
  });

  it('finds a key under its alias in the process and in a file', () => {
    const name = configFieldEnvName('tools.enableSecurity') as string;
    const alias = ALIASES.find(
      ([, current]) => current === name
    )?.[0] as string;
    const home = tempDir();
    const cwd = tempDir();
    expect(
      getKey(name, { env: { [alias]: 'pinned' }, home, cwd })
    ).toMatchObject({
      name,
      value: 'pinned',
      source: 'process',
    });
    expect(
      getKey(alias, { env: { [name]: 'pinned' }, home, cwd })
    ).toMatchObject({
      name,
      value: 'pinned',
      source: 'process',
    });
    fs.writeFileSync(path.join(home, '.env'), `${alias}=from-file\n`);
    expect(getKey(name, { env: {}, home, cwd })).toMatchObject({
      value: 'from-file',
      source: 'home',
    });
  });
});
