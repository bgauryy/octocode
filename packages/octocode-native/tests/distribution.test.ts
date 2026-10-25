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
      private?: boolean;
      version: string;
    };
    const { PLATFORMS, platformPackageName } = require(resolve(packageRoot, 'bin/platform.cjs')) as {
      PLATFORMS: Record<string, unknown>;
      platformPackageName(suffix: string): string;
    };

    // The workspace root is not published: the launchers depend on the platform packages.
    expect(manifest.private).toBe(true);
    expect(existsSync(resolve(packageRoot, 'js'))).toBe(false);

    for (const suffix of Object.keys(PLATFORMS)) {
      const platformManifest = require(resolve(packageRoot, 'npm', suffix, 'package.json')) as {
        name: string;
        version: string;
        main?: unknown;
        exports?: unknown;
        files: string[];
        preferUnplugged?: boolean;
      };
      expect(platformManifest.name).toBe(platformPackageName(suffix));
      expect(platformManifest.version).toBe(manifest.version);
      expect(platformManifest.main).toBeUndefined();
      expect(platformManifest.exports).toBeUndefined();
      expect(platformManifest.preferUnplugged).toBe(true);
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
