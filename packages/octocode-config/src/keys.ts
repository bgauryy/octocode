// The one API-key lookup for Octocode scripts and skills. A key comes from the first place
// that sets it: the process environment, then the home `.env` (global, `<home>/.env`), then
// the workspace `.env` (`<cwd>/.octocode/.env`). A repository's file therefore only fills a
// key the user has not set, and never sets a home-only or protected key. Only getKey returns
// a value; listings, setKey results and the CLI carry names, sources and paths.

import fs from 'node:fs';
import path from 'node:path';
import { API_KEYS, HOME_TRUSTED_ENV_KEYS, type ApiKeySpec } from './config/contract.generated.js';
import { CLASSIFICATION_KILL_SWITCH, fileEnv, isProtectedKey, sameFile } from './dotenv.js';
import { getOctocodeHome } from './home.js';

export type KeySource = 'process' | 'home' | 'workspace';
export type KeyScope = 'home' | 'workspace';

export interface KeyOptions {
  env?: Record<string, string | undefined>;
  /** The Octocode home; default `OCTOCODE_HOME` or `~/.octocode`. */
  home?: string;
  /** The workspace whose `.octocode/.env` is read; default the current directory. */
  cwd?: string;
}

/** The two files a lookup reads, after the process environment. */
export interface KeyFiles {
  home: string;
  workspace: string;
}

export interface KeyLookup {
  name: string;
  value?: string;
  source?: KeySource;
  files: KeyFiles;
}

export interface KeyStatus extends Omit<ApiKeySpec, 'name'> {
  name: string;
  key: 'set' | 'missing';
  source?: KeySource;
}

const HOME_TRUSTED: ReadonlySet<string> = new Set(HOME_TRUSTED_ENV_KEYS);

function context(options: KeyOptions) {
  const env = options.env ?? process.env;
  const home = options.home ?? getOctocodeHome(env);
  const cwd = options.cwd ?? process.cwd();
  return { env, files: { home: path.join(home, '.env'), workspace: path.join(cwd, '.octocode', '.env') } };
}

/** The files getKey reads for these options. */
export function keyFiles(options: KeyOptions = {}): KeyFiles {
  return context(options).files;
}

/** The value of `name` and where it came from: process, then home `.env`, then workspace `.env`. */
export function getKey(name: string, options: KeyOptions = {}): KeyLookup {
  const { env, files } = context(options);
  const fromProcess = env[name]?.trim() ? env[name] : undefined;
  if (fromProcess) return { name, value: fromProcess.trim(), source: 'process', files };
  // A present-but-blank classification key is the user's opt-out: no file refills it.
  if (name === CLASSIFICATION_KILL_SWITCH && env[name] !== undefined) return { name, files };
  // A protected key no file may set (PATH, proxies) comes only from the process.
  if (isProtectedKey(name) && !HOME_TRUSTED.has(name)) return { name, files };
  const fromHome = fileEnv(files.home)[name];
  if (fromHome) return { name, value: fromHome, source: 'home', files };
  // Home-only keys (endpoints, the classification key) never come from a repository.
  if (isProtectedKey(name) || sameFile(files.workspace, files.home)) return { name, files };
  const fromWorkspace = fileEnv(files.workspace)[name];
  if (fromWorkspace) return { name, value: fromWorkspace, source: 'workspace', files };
  return { name, files };
}

/** Every API key the contract declares (or the named ones), set or missing, with its source; never a value. */
export function listKeys(names?: readonly string[], options: KeyOptions = {}): { files: KeyFiles; keys: KeyStatus[] } {
  const specs = names?.length
    ? names.map(name => API_KEYS.find(spec => spec.name === name) ?? { name, dotenv: 'all' as const, description: 'Not a declared Octocode key' })
    : API_KEYS;
  const keys = specs.map(spec => {
    const { value, source } = getKey(spec.name, options);
    return { ...spec, key: value ? 'set' as const : 'missing' as const, ...(source ? { source } : {}) };
  });
  return { files: keyFiles(options), keys };
}

/**
 * Saves a declared API key to the home `.env` (default) or the workspace `.env`, replacing an
 * earlier line for it, with the file readable only by its owner. Returns the file and the source
 * getKey now answers from (the process environment still wins over both files).
 */
export function setKey(
  name: string,
  value: string,
  { scope = 'home', ...options }: KeyOptions & { scope?: KeyScope } = {}
): { name: string; scope: KeyScope; file: string; source?: KeySource } {
  const spec = API_KEYS.find(candidate => candidate.name === name);
  if (!spec) throw new Error(`${name} is not a declared Octocode API key (${API_KEYS.map(key => key.name).join(', ')})`);
  if (scope === 'workspace' && spec.dotenv === 'home') throw new Error(`${name} is home-only: a workspace .env cannot set it`);
  const clean = value.trim();
  if (!clean) throw new Error(`no value for ${name}`);
  if (/[\r\n]/.test(clean)) throw new Error(`${name} must be one line`);
  const files = keyFiles(options);
  const file = files[scope];
  fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
  const names = new Set([name]);
  let existing = '';
  try {
    existing = fs.readFileSync(file, 'utf8');
  } catch {
    // A missing file starts empty.
  }
  const kept = existing.split('\n').filter(line => {
    const entry = line.trim().replace(/^export\s+/, '');
    const eq = entry.indexOf('=');
    return eq === -1 || !names.has(entry.slice(0, eq).trim());
  });
  while (kept.length && !kept.at(-1)?.trim()) kept.pop();
  fs.writeFileSync(file, `${[...kept, `${name}=${clean}`].join('\n')}\n`, { mode: 0o600 });
  fs.chmodSync(file, 0o600);
  const { source } = getKey(name, options);
  return { name, scope, file, ...(source ? { source } : {}) };
}

const USAGE = `Usage: keys.mjs list [NAME...]
       keys.mjs get NAME...
       keys.mjs set NAME --stdin [--workspace]

A key comes from the first place that sets it: the process environment, then <home>/.env
(OCTOCODE_HOME, default ~/.octocode), then <workspace>/.octocode/.env (the current directory).
list: every declared API key (or the named ones), set or missing, with its source and docs link.
get:  the same for the named keys; exit 1 when any is missing.
set:  saves the value read from stdin to <home>/.env, or with --workspace to <workspace>/.octocode/.env.
No command prints a key value.`;

export interface KeysCliResult {
  exitCode: number;
  output: unknown;
}

/** The keys CLI: list, get and set. Returns the JSON to print and the exit code; never a value. */
export function runKeysCli(
  argv: readonly string[],
  { readStdin = () => fs.readFileSync(0, 'utf8'), ...options }: KeyOptions & { readStdin?: () => string } = {}
): KeysCliResult {
  const [command, ...rest] = argv;
  const fail = (code: string, message: string, exitCode = 2): KeysCliResult => ({ exitCode, output: { ok: false, error: { code, message } } });
  if (!command || command === '--help' || command === '-h') return { exitCode: command ? 0 : 2, output: USAGE };
  if (command === 'list' || command === 'get') {
    const names = rest.filter(arg => !arg.startsWith('-'));
    const unknown = rest.find(arg => arg.startsWith('-'));
    if (unknown) return fail('invalidArguments', `unknown option ${unknown}`);
    if (command === 'get' && !names.length) return fail('invalidArguments', 'get needs at least one key name');
    const { files, keys } = listKeys(names, options);
    const missing = keys.filter(key => key.key === 'missing');
    const ok = command === 'list' || missing.length === 0;
    return { exitCode: ok ? 0 : 1, output: { ok, files, keys: keys.map(key => key.key === 'missing' ? { ...key, fix: `keys.mjs set ${key.name} --stdin` } : key) } };
  }
  if (command === 'set') {
    const name = rest.find(arg => !arg.startsWith('-'));
    const flags = rest.filter(arg => arg.startsWith('-'));
    const badFlag = flags.find(flag => flag !== '--stdin' && flag !== '--workspace');
    if (badFlag) return fail('invalidArguments', `unknown option ${badFlag}`);
    if (!name) return fail('invalidArguments', 'set needs a key name');
    if (rest.filter(arg => !arg.startsWith('-')).length > 1) return fail('invalidArguments', 'pass the value on stdin with --stdin, never as an argument');
    if (!flags.includes('--stdin')) return fail('invalidArguments', 'set reads the value from stdin: pass --stdin and paste or pipe the key');
    try {
      const saved = setKey(name, readStdin(), { ...options, scope: flags.includes('--workspace') ? 'workspace' : 'home' });
      return { exitCode: 0, output: { ok: true, ...saved } };
    } catch (error) {
      return fail('invalidKey', error instanceof Error ? error.message : String(error));
    }
  }
  return fail('invalidArguments', `unknown command ${command} (list, get, set)`);
}
