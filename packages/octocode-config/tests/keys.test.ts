import { mkdtempSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { API_KEYS, HOME_TRUSTED_ENV_KEYS, LEGACY_ENV_NAMES, PROTECTED_KEY_NAMES, type ApiKeySpec } from '../src/config/contract.generated.js';
import { getKey, listKeys, runKeysCli, setKey } from '../src/index.js';

describe('getKey, listKeys, setKey and the keys CLI', () => {
  let root: string;
  let home: string;
  let cwd: string;
  const write = (file: string, text: string) => writeFileSync(file, text);
  const homeEnv = () => join(home, '.env');
  const workspaceEnv = () => join(cwd, '.octocode', '.env');

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'octocode-keys-'));
    home = join(root, 'home');
    cwd = join(root, 'project');
    mkdirSync(home);
    mkdirSync(join(cwd, '.octocode'), { recursive: true });
  });
  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it('reads process, then home, then workspace: the first place that sets the key wins', () => {
    const name = API_KEYS.find(key => key.dotenv === 'all')!.name;
    write(homeEnv(), `${name}=from-home\n`);
    write(workspaceEnv(), `${name}=from-workspace\n`);
    expect(getKey(name, { env: { [name]: 'from-process' }, home, cwd })).toMatchObject({ value: 'from-process', source: 'process' });
    expect(getKey(name, { env: {}, home, cwd })).toMatchObject({ value: 'from-home', source: 'home' });
    write(homeEnv(), '');
    expect(getKey(name, { env: {}, home, cwd })).toMatchObject({ value: 'from-workspace', source: 'workspace' });
    write(workspaceEnv(), `${name}=   \n`);
    expect(getKey(name, { env: { [name]: '  ' }, home, cwd })).toEqual({ name, files: { home: homeEnv(), workspace: workspaceEnv() } });
  });

  it('never takes a home-only key from a workspace, nor a protected key from any file', () => {
    const homeOnly = HOME_TRUSTED_ENV_KEYS[0];
    write(workspaceEnv(), `${homeOnly}=https://repository.example\n`);
    expect(getKey(homeOnly, { env: {}, home, cwd }).value).toBeUndefined();
    write(homeEnv(), `${homeOnly}=https://home.example\n`);
    expect(getKey(homeOnly, { env: {}, home, cwd })).toMatchObject({ value: 'https://home.example', source: 'home' });
    const processOnly = PROTECTED_KEY_NAMES.find(name => !(HOME_TRUSTED_ENV_KEYS as readonly string[]).includes(name))!;
    write(homeEnv(), `${processOnly}=/tmp/evil\n`);
    expect(getKey(processOnly, { env: {}, home, cwd }).value).toBeUndefined();
  });

  it('reads a legacy name as its current one', () => {
    const [legacy, current] = Object.entries(LEGACY_ENV_NAMES).find(([, now]) => API_KEYS.some(key => key.name === now))!;
    expect(getKey(current, { env: { [legacy]: 'old-spelling' }, home, cwd })).toMatchObject({ value: 'old-spelling', source: 'process' });
    write(homeEnv(), `${legacy}=old-in-file\n`);
    expect(getKey(current, { env: {}, home, cwd })).toMatchObject({ value: 'old-in-file', source: 'home' });
  });

  it('keeps a blank process classification key as the opt-out no file refills', () => {
    const classification = API_KEYS.find(key => key.dotenv === 'home')!.name;
    write(homeEnv(), `${classification}=home-value\n`);
    expect(getKey(classification, { env: { [classification]: '' }, home, cwd }).value).toBeUndefined();
  });

  it('lists every declared key with its status and source, and never a value', () => {
    const [first, second] = API_KEYS.filter(key => key.dotenv === 'all') as [ApiKeySpec, ApiKeySpec];
    write(homeEnv(), `${first.name}=secret-home\n`);
    write(workspaceEnv(), `${second.name}=secret-workspace\n`);
    const listed = listKeys(undefined, { env: {}, home, cwd });
    expect(listed.keys.map(key => key.name)).toEqual(API_KEYS.map(key => key.name));
    expect(listed.keys.find(key => key.name === first.name)).toMatchObject({ key: 'set', source: 'home' });
    expect(listed.keys.find(key => key.name === second.name)).toMatchObject({ key: 'set', source: 'workspace' });
    expect(JSON.stringify(listed)).not.toMatch(/secret/);
    expect(listKeys(['NOT_DECLARED'], { env: {}, home, cwd }).keys[0]).toMatchObject({ name: 'NOT_DECLARED', key: 'missing' });
  });

  it('setKey replaces the earlier line, keeps other lines, and leaves the file owner-only', () => {
    const name = API_KEYS.find(key => key.dotenv === 'all')!.name;
    write(homeEnv(), `OTHER=1\nexport ${name}=old\n# note\n`);
    expect(setKey(name, ' new-value \n', { env: {}, home, cwd })).toEqual({ name, scope: 'home', file: homeEnv(), source: 'home' });
    expect(readFileSync(homeEnv(), 'utf8')).toBe(`OTHER=1\n# note\n${name}=new-value\n`);
    expect(statSync(homeEnv()).mode & 0o777).toBe(0o600);
    expect(setKey(name, 'ws', { scope: 'workspace', env: { [name]: 'p' }, home, cwd })).toMatchObject({ file: workspaceEnv(), source: 'process' });
  });

  it('setKey refuses undeclared names, a home-only key in a workspace, and multi-line values', () => {
    const homeOnly = API_KEYS.find(key => key.dotenv === 'home')!.name;
    const any = API_KEYS.find(key => key.dotenv === 'all')!.name;
    expect(() => setKey('PATH', 'x', { home, cwd })).toThrow(/not a declared Octocode API key/);
    expect(() => setKey(homeOnly, 'x', { scope: 'workspace', home, cwd })).toThrow(/home-only/);
    expect(() => setKey(any, 'a\nb', { home, cwd })).toThrow(/one line/);
    expect(() => setKey(any, '  ', { home, cwd })).toThrow(/no value/);
  });

  it('the CLI lists, gets and sets without printing a value', () => {
    const name = API_KEYS.find(key => key.dotenv === 'all')!.name;
    const options = { env: {}, home, cwd };
    expect(runKeysCli(['get', name], options)).toMatchObject({ exitCode: 1, output: { ok: false, keys: [{ name, key: 'missing' }] } });
    const set = runKeysCli(['set', name, '--stdin', '--workspace'], { ...options, readStdin: () => 'cli-secret\n' });
    expect(set).toMatchObject({ exitCode: 0, output: { ok: true, scope: 'workspace', source: 'workspace' } });
    const got = runKeysCli(['get', name], options);
    expect(got).toMatchObject({ exitCode: 0, output: { ok: true, keys: [{ key: 'set', source: 'workspace' }] } });
    expect(runKeysCli(['list'], options).exitCode).toBe(0);
    expect(JSON.stringify([set, got, runKeysCli(['list'], options)])).not.toMatch(/cli-secret/);
    expect(runKeysCli(['set', name, 'inline-secret'], options)).toMatchObject({ exitCode: 2, output: { error: { code: 'invalidArguments' } } });
    expect(runKeysCli(['set', name], options).exitCode).toBe(2);
    expect(runKeysCli(['nope'], options).exitCode).toBe(2);
    expect(runKeysCli(['--help'], options)).toMatchObject({ exitCode: 0, output: expect.stringMatching(/^Usage: keys\.mjs/) });
  });
});
