import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

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

  it('preserves root, runtime, legacy, and compatibility identities', () => {
    const root = require(resolve(packageRoot));
    const runtime = require(resolve(packageRoot, 'js/runtime.cjs'));
    const legacy = require(resolve(packageRoot, 'native.cjs'));
    const engine = require(resolve(packageRoot, 'js/engine.cjs'));
    const compatibility = require(resolve(packageRoot, '../octocode-engine'));

    expect(root.NativeRuntime).toBe(runtime.NativeRuntime);
    expect(legacy.NativeRuntime).toBe(runtime.NativeRuntime);
    const runtimeInstance = new runtime.NativeRuntime();
    expect(runtimeInstance.abiVersion).toBe(2);
    runtimeInstance.close();
    expect(compatibility.minifyContent).toBe(engine.minifyContent);
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
