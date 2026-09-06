import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

interface PackageManifest {
  version: string;
  bin: string;
  main: string;
  exports: {
    '.': {
      types: string;
      import: string;
    };
  };
  files: string[];
}

interface DxtManifest {
  version: string;
  icon: string;
  tools: Array<{ name: string }>;
  server: { mcp_config: { env: Record<string, string> } };
}

interface McpRegistryManifest {
  version: string;
  packages: Array<{
    identifier: string;
    version: string;
  }>;
}

const packageRoot = resolve(import.meta.dirname, '..');

function readJson<T>(relativePath: string): T {
  return JSON.parse(
    readFileSync(resolve(packageRoot, relativePath), 'utf8')
  ) as T;
}

describe('published package release contract', () => {
  const packageJson = readJson<PackageManifest>('package.json');
  const dxtManifest = readJson<DxtManifest>('manifest.json');
  const registryManifest = readJson<McpRegistryManifest>('server.json');

  it('wires desktop user settings to the runtime environment', () => {
    expect(dxtManifest.server.mcp_config.env).toMatchObject({
      GITHUB_TOKEN: '${user_config.github_token}',
      NPM_CONFIG_REGISTRY: '${user_config.npm_registry}',
    });
  });

  it('advertises canonical MCP tool names without client prefixes', async () => {
    const { DIRECT_TOOL_DEFINITIONS } =
      await import('@octocodeai/octocode-tools-core/schema');
    expect(dxtManifest.tools.map(tool => tool.name).sort()).toEqual(
      DIRECT_TOOL_DEFINITIONS.map(tool => tool.name).sort()
    );
  });

  it('keeps all published release versions synchronized', () => {
    expect(dxtManifest.version).toBe(packageJson.version);
    expect(registryManifest.version).toBe(packageJson.version);
    expect(registryManifest.packages).toContainEqual(
      expect.objectContaining({
        identifier: 'octocode-mcp',
        version: packageJson.version,
      })
    );
  });

  it('ships the DXT icon at the declared path', () => {
    expect(dxtManifest.icon).toBe('assets/logo.png');
    expect(packageJson.files).toContain(dxtManifest.icon);
    expect(existsSync(resolve(packageRoot, dxtManifest.icon))).toBe(true);
  });

  it('keeps the root programmatic import aligned with its declarations', () => {
    expect(packageJson.exports['.']).toEqual({
      types: './dist/public.d.ts',
      import: './dist/public.js',
    });
    expect(packageJson.main).toBe('dist/public.js');
    expect(packageJson.bin).toBe('dist/index.js');
  });
});
