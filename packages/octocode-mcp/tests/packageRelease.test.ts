import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

interface PackageManifest {
  version: string;
  bin: string;
  main?: unknown;
  exports?: unknown;
  files: string[];
  dependencies: Record<string, string>;
  optionalDependencies: Record<string, string>;
}

interface DxtManifest {
  version: string;
  icon: string;
  tools: Array<{ name: string; description: string }>;
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
    });
  });

  it('advertises canonical MCP tool names without client prefixes', async () => {
    const { DIRECT_TOOL_DEFINITIONS } =
      await import('@octocodeai/config/schema');
    // The manifest lists every tool MCP can register, opt-in tools included:
    // ghCloneRepo stays here and is hidden at runtime unless tools.enableClone is on.
    expect(dxtManifest.tools.map(tool => tool.name).sort()).toEqual(
      DIRECT_TOOL_DEFINITIONS.map(tool => tool.name).sort()
    );
  });

  it('advertises the API prerequisite for clasify independently of beta', async () => {
    const { configFieldEnvName } = await import('@octocodeai/config');
    const { getPublicToolCatalog, TOOL_NAMES } =
      await import('@octocodeai/config/schema');
    const descriptions = new Map(
      dxtManifest.tools.map(tool => [tool.name, tool.description])
    );
    const betaGate = `beta: requires ${configFieldEnvName('local.beta')}`;
    const apiEnv = configFieldEnvName('classification.api');

    expect(descriptions.get(TOOL_NAMES.CLASIFY)).toContain(
      `(conditional: requires ${apiEnv})`
    );
    expect(descriptions.get(TOOL_NAMES.CLASIFY)).not.toContain(
      configFieldEnvName('local.beta')
    );
    for (const tool of getPublicToolCatalog().tools) {
      if (
        !tool.beta ||
        typeof tool.name !== 'string' ||
        tool.name === TOOL_NAMES.CLASIFY
      )
        continue;
      expect(descriptions.get(tool.name)).toContain(`(${betaGate})`);
    }
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
});

describe('published launcher package', () => {
  const packageJson = readJson<PackageManifest>('package.json');

  it('is a launcher: one executable, no library entry, the binary from platform packages', () => {
    expect(packageJson.bin).toBe('dist/index.js');
    expect(packageJson.main).toBeUndefined();
    expect(packageJson.exports).toBeUndefined();
    expect(Object.keys(packageJson.dependencies)).toEqual([
      '@typescript/native-preview',
    ]);
    expect(
      Object.keys(packageJson.optionalDependencies).length
    ).toBeGreaterThan(0);
    for (const [name, version] of Object.entries(
      packageJson.optionalDependencies
    )) {
      expect(name).toMatch(/^@octocodeai\/octocode-native-/);
      expect(version).toBe(packageJson.version);
    }
  });
});
