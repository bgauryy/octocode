// Global + workspace `.octocoderc` layering (parity with the native resolver):
// process env > workspace .env > global .env > workspace .octocoderc
// > global .octocoderc > defaults — resolved per field.
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  getProjectConfigFilePath,
  isPersistentStorageEnabled,
  isPersistentStorageEnabledForExtension,
  loadOctocodercLayers,
  loadProjectConfigSync,
  resolveConfigFields,
} from '../src/index.js';

let root: string;
let home: string;
let cwd: string;
const saved = { ...process.env };

function writeGlobal(value: unknown | string): void {
  writeFileSync(
    join(home, '.octocoderc'),
    typeof value === 'string' ? value : JSON.stringify(value)
  );
}
function writeWorkspace(value: unknown | string): void {
  writeFileSync(
    join(cwd, '.octocode', '.octocoderc'),
    typeof value === 'string' ? value : JSON.stringify(value)
  );
}
const env = () => ({ OCTOCODE_HOME: home });
const resolve = (extraEnv: Record<string, string> = {}) =>
  resolveConfigFields(
    loadOctocodercLayers({ env: env(), cwd }) as never,
    extraEnv
  );

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'octo-layers-'));
  home = join(root, 'home');
  cwd = join(root, 'repo');
  mkdirSync(home, { recursive: true });
  mkdirSync(join(cwd, '.octocode'), { recursive: true });
  delete process.env['OCTOCODE_STORAGE_MODE'];
  delete process.env['OCTOCODE_EXTENSION_STORAGE_MODE'];
  process.env['OCTOCODE_HOME'] = home;
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
  for (const key of Object.keys(process.env))
    if (!(key in saved)) delete process.env[key];
  Object.assign(process.env, saved);
  vi.restoreAllMocks();
});

describe('workspace .octocoderc path + loader', () => {
  it('lives at <cwd>/.octocode/.octocoderc', () => {
    expect(getProjectConfigFilePath(cwd)).toBe(join(cwd, '.octocode', '.octocoderc'));
    expect(loadProjectConfigSync(cwd)).toMatchObject({
      success: false,
      error: 'Config file does not exist',
    });
    writeWorkspace('{ // comment\n "network": { "timeout": 7000, },\n}');
    expect(loadProjectConfigSync(cwd)).toMatchObject({
      success: true,
      config: { network: { timeout: 7000 } },
    });
  });

  it('returns layers workspace-first, {} for absent files', () => {
    expect(loadOctocodercLayers({ env: env(), cwd })).toEqual([{}, {}]);
    writeGlobal({ a: 1 });
    writeWorkspace({ b: 2 });
    expect(loadOctocodercLayers({ env: env(), cwd })).toEqual([{ b: 2 }, { a: 1 }]);
  });

  it('reads the home file once when the workspace dir IS the Octocode home', () => {
    writeGlobal({ a: 1 });
    const osHome = join(root, 'user');
    mkdirSync(osHome);
    symlinkSync(home, join(osHome, '.octocode'));
    // Same directory via a symlinked path.
    expect(loadOctocodercLayers({ env: env(), cwd: osHome })).toEqual([{ a: 1 }]);
    // Same directory via an identical path.
    expect(
      loadOctocodercLayers({ env: { OCTOCODE_HOME: join(cwd, '.octocode') }, cwd })
    ).toHaveLength(1);
  });

  it('never throws on a broken file: warns with the path and ignores that file only', () => {
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    writeGlobal({ network: { timeout: 6000 } });
    writeWorkspace('{broken');
    expect(() => loadOctocodercLayers({ env: env(), cwd })).not.toThrow();
    expect(resolve().network.timeout).toBe(6000);
    const printed = stderr.mock.calls.map(call => String(call[0])).join('');
    expect(printed).toContain(join(cwd, '.octocode', '.octocoderc'));
    expect(printed).toContain('warning');
    expect(printed).toContain('whole file is ignored');
  });
});

describe('per-field resolution across layers', () => {
  it('workspace wins per field; global fills the rest', () => {
    writeGlobal({ network: { timeout: 6000, maxRetries: 1 }, output: { format: 'json' } });
    writeWorkspace({ network: { timeout: 7000 } });
    const resolved = resolve();
    expect(resolved.network.timeout).toBe(7000);
    expect(resolved.network.maxRetries).toBe(1);
    expect(resolved.output.format).toBe('json');
  });

  it('workspace alone applies', () => {
    writeWorkspace({ network: { timeout: 7000 } });
    expect(resolve().network.timeout).toBe(7000);
  });

  it('environment beats both files', () => {
    writeGlobal({ network: { timeout: 6000 } });
    writeWorkspace({ network: { timeout: 7000 } });
    expect(resolve({ REQUEST_TIMEOUT: '9000' }).network.timeout).toBe(9000);
  });

  it('an invalid workspace value falls through to the global value', () => {
    writeGlobal({ output: { format: 'json' }, network: { timeout: 6000 } });
    writeWorkspace({ output: { format: 'xml' }, network: { timeout: 'slow' } });
    const resolved = resolve();
    expect(resolved.output.format).toBe('json');
    expect(resolved.network.timeout).toBe(6000);
  });

  it('workspace null resets a global list; arrays replace, never concatenate', () => {
    writeGlobal({ tools: { enabled: ['localSearch'], disabled: ['ghSearchCode', 'astSearch'] } });
    writeWorkspace({ tools: { enabled: null, disabled: ['lspSearch'] } });
    const resolved = resolve();
    expect(resolved.tools.enabled).toBeNull();
    expect(resolved.tools.disabled).toEqual(['lspSearch']);
  });

  it('a single object is still accepted (backward compatible)', () => {
    expect(resolveConfigFields({ network: { timeout: 7000 } }, {}).network.timeout).toBe(7000);
  });
});

describe('workspace trust boundary', () => {
  it('drops protected fields from the workspace layer only, with a warning', () => {
    const stderr = vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    writeWorkspace({
      github: { apiUrl: 'https://evil.example/api' },
      local: { allowedPaths: ['/'] },
      lsp: { configPath: '/repo/evil.json' },
      output: { format: 'json' },
    });
    const resolved = resolve();
    expect(resolved.github.apiUrl).toBe('https://api.github.com');
    expect(resolved.local.allowedPaths).toEqual([]);
    expect(resolved.lsp.configPath ?? null).toBeNull();
    expect(resolved.output.format).toBe('json');
    const printed = stderr.mock.calls.map(call => String(call[0])).join('');
    for (const field of ['github.apiUrl', 'local.allowedPaths', 'lsp.configPath'])
      expect(printed).toContain(field);
    expect(printed).toContain(join(cwd, '.octocode', '.octocoderc'));

    writeGlobal({ github: { apiUrl: 'https://ghe.example/api/v3' } });
    expect(resolve().github.apiUrl).toBe('https://ghe.example/api/v3');
  });
});

describe('storage helpers honor the workspace layer', () => {
  it('workspace storage.mode beats global', () => {
    writeGlobal({ storage: { mode: 'persistent' } });
    writeWorkspace({ storage: { mode: 'memory' } });
    expect(isPersistentStorageEnabled(process.env, cwd)).toBe(false);
    expect(isPersistentStorageEnabledForExtension(cwd)).toBe(false);
  });

  it('global applies when the workspace file is silent or invalid', () => {
    vi.spyOn(process.stderr, 'write').mockReturnValue(true);
    writeGlobal({ storage: { mode: 'memory' } });
    writeWorkspace({ output: { format: 'json' } });
    expect(isPersistentStorageEnabled(process.env, cwd)).toBe(false);
    writeWorkspace('{broken');
    expect(isPersistentStorageEnabled(process.env, cwd)).toBe(false);
  });

  it('extension mode: any layer extension.storage beats any layer storage', () => {
    writeGlobal({ extension: { storage: { mode: 'persistent' } } });
    writeWorkspace({ storage: { mode: 'memory' } });
    expect(isPersistentStorageEnabledForExtension(cwd)).toBe(true);
    expect(isPersistentStorageEnabled(process.env, cwd)).toBe(false);
  });

  it('env beats every layer', () => {
    writeWorkspace({ storage: { mode: 'memory' } });
    process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';
    expect(isPersistentStorageEnabled(process.env, cwd)).toBe(true);
    expect(isPersistentStorageEnabledForExtension(cwd)).toBe(true);
  });
});
