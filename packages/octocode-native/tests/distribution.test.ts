import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { NATIVE_ABI_VERSION } from '../js/runtime.js';

const require = createRequire(import.meta.url);
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function loadedAddons(entry: string): string[] {
  const output = execFileSync(
    process.execPath,
    [
      '-e',
      "require(process.argv[1]); process.stdout.write(JSON.stringify(Object.keys(require.cache).filter(path => path.endsWith('.node')).map(path => require('node:path').basename(path))))",
      entry,
    ],
    { cwd: packageRoot, encoding: 'utf8' }
  );
  return JSON.parse(output) as string[];
}

describe('consolidated native distribution', () => {
  it('keeps runtime and engine addon loading independent', () => {
    const runtimeAddons = loadedAddons(resolve(packageRoot, 'js/runtime.cjs'));
    const engineAddons = loadedAddons(resolve(packageRoot, 'js/engine.cjs'));

    expect(runtimeAddons.some(name => name.startsWith('octocode-native.'))).toBe(true);
    expect(runtimeAddons.some(name => name.startsWith('octocode-engine.'))).toBe(false);
    expect(engineAddons.some(name => name.startsWith('octocode-engine.'))).toBe(true);
    expect(engineAddons.some(name => name.startsWith('octocode-native.'))).toBe(false);
  });

  it('preserves root, runtime, legacy-loader, and engine identities', () => {
    const root = require(resolve(packageRoot));
    const runtime = require(resolve(packageRoot, 'js/runtime.cjs'));
    const legacy = require(resolve(packageRoot, 'native.cjs'));
    const engine = require(resolve(packageRoot, 'js/engine.cjs'));

    expect(root.NativeRuntime).toBe(runtime.NativeRuntime);
    expect(legacy.NativeRuntime).toBe(runtime.NativeRuntime);
    expect(engine.minifyContent).toBeTypeOf('function');
    expect(existsSync(resolve(packageRoot, '../octocode-engine'))).toBe(false);
    const runtimeInstance = new runtime.NativeRuntime();
    expect(runtimeInstance.abiVersion).toBe(NATIVE_ABI_VERSION);
    runtimeInstance.close();
  });

  it('loads the exact staged host addons and verifies Darwin signatures', () => {
    const { getPlatformSuffix } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      getPlatformSuffix(): string;
    };
    const suffix = getPlatformSuffix();
    const runtimePath = resolve(packageRoot, 'npm', suffix, `octocode-native.${suffix}.node`);
    const enginePath = resolve(packageRoot, 'npm', suffix, `octocode-engine.${suffix}.node`);

    for (const addonPath of [runtimePath, enginePath]) {
      expect(existsSync(addonPath)).toBe(true);
      expect(() => require(addonPath)).not.toThrow();
      if (process.platform === 'darwin') {
        expect(() => execFileSync('codesign', ['--verify', '--strict', addonPath])).not.toThrow();
      }
    }
    const runtime = require(runtimePath) as { NativeRuntime: new () => { abiVersion: number; close(): void } };
    const instance = new runtime.NativeRuntime();
    expect(instance.abiVersion).toBe(NATIVE_ABI_VERSION);
    instance.close();
  });

  it('declares four-artifact platform contracts', () => {
    const manifest = require(resolve(packageRoot, 'package.json')) as {
      optionalDependencies: Record<string, string>;
      exports: Record<string, unknown>;
      version: string;
    };

    expect(Object.keys(manifest.optionalDependencies)).toHaveLength(6);
    expect(manifest.exports).toHaveProperty('.');
    expect(manifest.exports).toHaveProperty('./runtime');
    expect(manifest.exports).toHaveProperty('./engine');

    for (const packageName of Object.keys(manifest.optionalDependencies)) {
      const suffix = packageName.replace('@octocodeai/octocode-native-', '');
      const platformManifest = require(resolve(packageRoot, 'npm', suffix, 'package.json')) as {
        version: string;
        exports: Record<string, unknown>;
      };
      expect(platformManifest.version).toBe(manifest.version);
      expect(platformManifest.exports).toHaveProperty('./runtime');
      expect(platformManifest.exports).toHaveProperty('./engine');
    }
  });
});
