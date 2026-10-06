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
  it('loads only the runtime addon', () => {
    const runtimeAddons = loadedAddons(resolve(packageRoot, 'js/runtime.cjs'));

    expect(runtimeAddons).toHaveLength(1);
    expect(runtimeAddons[0]).toMatch(/^octocode-native\./);
  });

  it('preserves root and runtime identities', () => {
    const root = require(resolve(packageRoot));
    const runtime = require(resolve(packageRoot, 'js/runtime.cjs'));

    expect(root.NativeRuntime).toBe(runtime.NativeRuntime);
    expect(existsSync(resolve(packageRoot, '../octocode-engine'))).toBe(false);
    const runtimeInstance = new runtime.NativeRuntime();
    expect(runtimeInstance.abiVersion).toBe(NATIVE_ABI_VERSION);
    runtimeInstance.close();
  });

  it('loads the exact staged host addon and verifies Darwin signatures', () => {
    const { getPlatformSuffix } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      getPlatformSuffix(): string;
    };
    const suffix = getPlatformSuffix();
    const runtimePath = resolve(packageRoot, 'npm', suffix, `octocode-native.${suffix}.node`);

    expect(existsSync(runtimePath)).toBe(true);
    if (process.platform === 'darwin') {
      expect(() => execFileSync('codesign', ['--verify', '--strict', runtimePath])).not.toThrow();
    }
    const runtime = require(runtimePath) as { NativeRuntime: new () => { abiVersion: number; close(): void } };
    const instance = new runtime.NativeRuntime();
    expect(instance.abiVersion).toBe(NATIVE_ABI_VERSION);
    instance.close();
  });

  it('declares three-artifact platform contracts', () => {
    const manifest = require(resolve(packageRoot, 'package.json')) as {
      optionalDependencies: Record<string, string>;
      exports: Record<string, unknown>;
      version: string;
    };

    const { PLATFORMS } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      PLATFORMS: Record<string, unknown>;
    };

    expect(Object.keys(manifest.optionalDependencies)).toHaveLength(Object.keys(PLATFORMS).length);
    expect(manifest.exports).toHaveProperty('.');
    expect(manifest.exports).toHaveProperty('./runtime');

    for (const packageName of Object.keys(manifest.optionalDependencies)) {
      const suffix = packageName.replace('@octocodeai/octocode-native-', '');
      const platformManifest = require(resolve(packageRoot, 'npm', suffix, 'package.json')) as {
        version: string;
        exports: Record<string, unknown>;
      };
      expect(platformManifest.version).toBe(manifest.version);
      expect(platformManifest.exports).toHaveProperty('./runtime');
    }
  });
});
