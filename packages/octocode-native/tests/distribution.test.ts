import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const require = createRequire(import.meta.url);
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

describe('native distribution', () => {
  it('ships one binary per platform and no Node addon', () => {
    const manifest = require(resolve(packageRoot, 'package.json')) as {
      optionalDependencies: Record<string, string>;
      exports: Record<string, unknown>;
      main?: unknown;
      version: string;
    };
    const { PLATFORMS } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      PLATFORMS: Record<string, unknown>;
    };

    expect(Object.keys(manifest.optionalDependencies)).toHaveLength(Object.keys(PLATFORMS).length);
    expect(manifest.main).toBeUndefined();
    expect(Object.keys(manifest.exports).every(key => key.startsWith('./bin/') || key === './package.json')).toBe(true);
    expect(existsSync(resolve(packageRoot, 'js'))).toBe(false);

    for (const packageName of Object.keys(manifest.optionalDependencies)) {
      const suffix = packageName.replace('@octocodeai/octocode-native-', '');
      const platformManifest = require(resolve(packageRoot, 'npm', suffix, 'package.json')) as {
        version: string;
        main?: unknown;
        exports?: unknown;
        files: string[];
      };
      expect(platformManifest.version).toBe(manifest.version);
      expect(platformManifest.main).toBeUndefined();
      expect(platformManifest.exports).toBeUndefined();
      expect(platformManifest.files.some(file => file.endsWith('.node'))).toBe(false);
    }
  });

  it('stages a host binary that serves MCP', () => {
    const { getPlatformSuffix, executableName } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      getPlatformSuffix(): string;
      executableName(binary: string): string;
    };
    const binary = resolve(packageRoot, 'npm', getPlatformSuffix(), executableName('octocode'));

    expect(existsSync(binary)).toBe(true);
    expect(execFileSync(binary, ['mcp', '--help'], { encoding: 'utf8' })).toContain('MCP server');
  });
});
