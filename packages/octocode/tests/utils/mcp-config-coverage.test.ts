import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { MCPConfig } from '../../src/types/index.js';
import type { MCPRegistryEntry } from '../../src/utils/mcp-config.js';

vi.mock('../../src/utils/platform.js', () => ({
  isWindows: false,
}));

vi.mock('../../src/utils/mcp-paths.js', () => ({
  getMCPConfigPath: vi.fn(),
  clientConfigExists: vi.fn(),
  configFileExists: vi.fn(),
  detectCurrentClient: vi.fn(),
  detectAvailableClients: vi.fn(),
  MCP_CLIENTS: {},
}));

vi.mock('../../src/utils/mcp-io.js', () => ({
  readMCPConfig: vi.fn(),
  writeMCPConfig: vi.fn(),
}));

describe('MCP Config Coverage Tests', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('getOctocodeServerConfig with env options', () => {
    it('should include ENABLE_LOCAL env when enableLocal is true', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx', { enableLocal: true });

      expect(result.env).toBeDefined();
      expect(result.env!.ENABLE_LOCAL).toBe('true');
    });

    it('should include ENABLE_LOCAL=false env when enableLocal is false', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx', { enableLocal: false });

      expect(result.env).toBeDefined();
      expect(result.env!.ENABLE_LOCAL).toBe('false');
    });

    it('should include GITHUB_TOKEN env when githubToken is provided', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx', {
        githubToken: 'ghp_testtoken123',
      });

      expect(result.env).toBeDefined();
      expect(result.env!.GITHUB_TOKEN).toBe('ghp_testtoken123');
    });

    it('should include both env vars when both options are provided', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx', {
        enableLocal: true,
        githubToken: 'ghp_testtoken123',
      });

      expect(result.env).toBeDefined();
      expect(result.env!.ENABLE_LOCAL).toBe('true');
      expect(result.env!.GITHUB_TOKEN).toBe('ghp_testtoken123');
    });

    it('should not include env when envOptions is undefined', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx');

      expect(result.env).toBeUndefined();
    });

    it('should not include env when envOptions is empty object', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx', {});

      expect(result.env).toBeUndefined();
    });

    it('should throw for removed direct method', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      expect(() =>
        getOctocodeServerConfig('direct' as any, {
          enableLocal: true,
          githubToken: 'ghp_token',
        })
      ).toThrow('Unknown install method');
    });
  });

  describe('getOctocodeServerConfigWindows with env options', () => {
    it('should delegate to getOctocodeServerConfig for npx method with env', async () => {
      const { getOctocodeServerConfigWindows } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfigWindows('npx', {
        enableLocal: true,
      });

      expect(result.command).toBe('npx');
      expect(result.env!.ENABLE_LOCAL).toBe('true');
    });
  });

  describe('mergeOctocodeConfig with env options', () => {
    it('should merge config with env options', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeOctocodeConfig({ mcpServers: {} }, 'npx', {
        enableLocal: true,
        githubToken: 'ghp_merge_token',
      });

      expect(result.mcpServers!.octocode.env).toBeDefined();
      expect(result.mcpServers!.octocode.env!.ENABLE_LOCAL).toBe('true');
      expect(result.mcpServers!.octocode.env!.GITHUB_TOKEN).toBe(
        'ghp_merge_token'
      );
    });

    it('should merge config with partial env options', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeOctocodeConfig({ mcpServers: {} }, 'npx', {
        enableLocal: true,
      });

      expect(result.mcpServers!.octocode.env!.ENABLE_LOCAL).toBe('true');
      expect(result.mcpServers!.octocode.env!.GITHUB_TOKEN).toBeUndefined();
    });

    it('should handle null config.mcpServers', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeOctocodeConfig({} as MCPConfig, 'npx');

      expect(result.mcpServers!.octocode).toBeDefined();
    });
  });

  describe('registryEntryToServerConfig', () => {
    const mockEntry: MCPRegistryEntry = {
      id: 'test-mcp',
      name: 'Test MCP',
      description: 'A test MCP server',
      category: 'developer-tools',
      repository: 'https://github.com/test/test-mcp',
      installationType: 'npx',
      installConfig: {
        command: 'node',
        args: ['--api-key', '${API_KEY}', '--endpoint', '${ENDPOINT}'],
      },
      requiredEnvVars: [
        { name: 'API_KEY', description: 'API key' },
        { name: 'ENDPOINT', description: 'Endpoint URL' },
      ],
    };

    it('should convert registry entry to server config without env values', async () => {
      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(mockEntry);

      expect(result.command).toBe('node');
      expect(result.args).toEqual([
        '--api-key',
        '${API_KEY}',
        '--endpoint',
        '${ENDPOINT}',
      ]);
      expect(result.env).toBeUndefined();
    });

    it('should replace placeholders with env values', async () => {
      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(mockEntry, {
        API_KEY: 'my-api-key',
        ENDPOINT: 'https://api.example.com',
      });

      expect(result.args).toEqual([
        '--api-key',
        'my-api-key',
        '--endpoint',
        'https://api.example.com',
      ]);
    });

    it('should keep placeholder if env value is missing', async () => {
      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(mockEntry, {
        API_KEY: 'my-api-key',
      });

      expect(result.args).toContain('my-api-key');
      expect(result.args).toContain('${ENDPOINT}');
    });

    it('should merge env from installConfig and provided values', async () => {
      const entryWithEnv: MCPRegistryEntry = {
        ...mockEntry,
        installConfig: {
          command: 'node',
          args: ['script.js'],
          env: {
            DEFAULT_VAR: 'default-value',
          },
        },
      };

      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(entryWithEnv, {
        API_KEY: 'user-api-key',
      });

      expect(result.env).toBeDefined();
      expect(result.env!.DEFAULT_VAR).toBe('default-value');
      expect(result.env!.API_KEY).toBe('user-api-key');
    });

    it('should not add empty env values', async () => {
      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(mockEntry, {
        API_KEY: 'valid-key',
        EMPTY_VAR: '',
      });

      expect(result.env!.API_KEY).toBe('valid-key');
      expect(result.env!.EMPTY_VAR).toBeUndefined();
    });

    it('should handle entry without installConfig.env', async () => {
      const { registryEntryToServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = registryEntryToServerConfig(mockEntry, {
        API_KEY: 'key123',
      });

      expect(result.env!.API_KEY).toBe('key123');
    });
  });

  describe('mergeExternalMCPConfig', () => {
    const mockEntry: MCPRegistryEntry = {
      id: 'external-server',
      name: 'External Server',
      description: 'An external MCP server',
      category: 'developer-tools',
      repository: 'https://github.com/test/external-server',
      installationType: 'npx',
      installConfig: {
        command: 'npx',
        args: ['external-mcp@latest'],
      },
    };

    it('should add external MCP to empty config', async () => {
      const { mergeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeExternalMCPConfig({ mcpServers: {} }, mockEntry);

      expect(result.mcpServers!['external-server']).toBeDefined();
      expect(result.mcpServers!['external-server'].command).toBe('npx');
    });

    it('should preserve existing servers when adding external MCP', async () => {
      const { mergeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const existing: MCPConfig = {
        mcpServers: {
          octocode: { command: 'npx', args: ['octocode-mcp@latest'] },
        },
      };

      const result = mergeExternalMCPConfig(existing, mockEntry);

      expect(result.mcpServers!.octocode).toBeDefined();
      expect(result.mcpServers!['external-server']).toBeDefined();
    });

    it('should overwrite existing external MCP with same id', async () => {
      const { mergeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const existing: MCPConfig = {
        mcpServers: {
          'external-server': { command: 'old-command', args: [] },
        },
      };

      const result = mergeExternalMCPConfig(existing, mockEntry);

      expect(result.mcpServers!['external-server'].command).toBe('npx');
    });

    it('should pass env values to registryEntryToServerConfig', async () => {
      const entryWithEnv: MCPRegistryEntry = {
        ...mockEntry,
        installConfig: {
          command: 'node',
          args: ['--token', '${TOKEN}'],
        },
      };

      const { mergeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeExternalMCPConfig({ mcpServers: {} }, entryWithEnv, {
        TOKEN: 'secret-token',
      });

      expect(result.mcpServers!['external-server'].args).toContain(
        'secret-token'
      );
    });
  });

  describe('isExternalMCPConfigured', () => {
    it('should return true when external MCP is configured', async () => {
      const { isExternalMCPConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'my-external-mcp': { command: 'npx', args: [] },
        },
      };

      expect(isExternalMCPConfigured(config, 'my-external-mcp')).toBe(true);
    });

    it('should return false when external MCP is not configured', async () => {
      const { isExternalMCPConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      };

      expect(isExternalMCPConfigured(config, 'my-external-mcp')).toBe(false);
    });

    it('should return false when mcpServers is undefined', async () => {
      const { isExternalMCPConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config = {} as MCPConfig;

      expect(isExternalMCPConfigured(config, 'my-external-mcp')).toBe(false);
    });

    it('should return false when mcpServers is empty', async () => {
      const { isExternalMCPConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = { mcpServers: {} };

      expect(isExternalMCPConfigured(config, 'my-external-mcp')).toBe(false);
    });
  });

  describe('removeExternalMCPConfig', () => {
    it('should remove existing external MCP', async () => {
      const { removeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'external-mcp': { command: 'npx', args: [] },
          octocode: { command: 'npx', args: ['octocode-mcp@latest'] },
        },
      };

      const result = removeExternalMCPConfig(config, 'external-mcp');

      expect(result.mcpServers!['external-mcp']).toBeUndefined();
      expect(result.mcpServers!.octocode).toBeDefined();
    });

    it('should return same config when entryId does not exist', async () => {
      const { removeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      };

      const result = removeExternalMCPConfig(config, 'non-existent');

      expect(result).toEqual(config);
    });

    it('should return same config when mcpServers is undefined', async () => {
      const { removeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const config = {} as MCPConfig;

      const result = removeExternalMCPConfig(config, 'any-id');

      expect(result).toEqual(config);
    });

    it('should return empty mcpServers when removing last server', async () => {
      const { removeExternalMCPConfig } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'only-server': { command: 'npx', args: [] },
        },
      };

      const result = removeExternalMCPConfig(config, 'only-server');

      expect(result.mcpServers).toEqual({});
    });
  });

  describe('getInstalledExternalMCPs', () => {
    const registry: MCPRegistryEntry[] = [
      {
        id: 'mcp-1',
        name: 'MCP 1',
        description: 'First MCP',
        category: 'developer-tools',
        repository: 'https://github.com/test/mcp-1',
        installationType: 'npx',
        installConfig: { command: 'npx', args: ['mcp-1'] },
      },
      {
        id: 'mcp-2',
        name: 'MCP 2',
        description: 'Second MCP',
        category: 'developer-tools',
        repository: 'https://github.com/test/mcp-2',
        installationType: 'npx',
        installConfig: { command: 'npx', args: ['mcp-2'] },
      },
      {
        id: 'mcp-3',
        name: 'MCP 3',
        description: 'Third MCP',
        category: 'developer-tools',
        repository: 'https://github.com/test/mcp-3',
        installationType: 'npx',
        installConfig: { command: 'npx', args: ['mcp-3'] },
      },
    ];

    it('should return installed MCPs that are in registry', async () => {
      const { getInstalledExternalMCPs } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'mcp-1': { command: 'npx', args: [] },
          'mcp-3': { command: 'npx', args: [] },
        },
      };

      const result = getInstalledExternalMCPs(config, registry);

      expect(result.length).toBe(2);
      expect(result.map(e => e.id)).toContain('mcp-1');
      expect(result.map(e => e.id)).toContain('mcp-3');
      expect(result.map(e => e.id)).not.toContain('mcp-2');
    });

    it('should return empty array when no MCPs are installed', async () => {
      const { getInstalledExternalMCPs } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = { mcpServers: {} };

      const result = getInstalledExternalMCPs(config, registry);

      expect(result).toEqual([]);
    });

    it('should return empty array when mcpServers is undefined', async () => {
      const { getInstalledExternalMCPs } =
        await import('../../src/utils/mcp-config.js');

      const config = {} as MCPConfig;

      const result = getInstalledExternalMCPs(config, registry);

      expect(result).toEqual([]);
    });

    it('should not return installed MCPs not in registry', async () => {
      const { getInstalledExternalMCPs } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'mcp-1': { command: 'npx', args: [] },
          'unknown-mcp': { command: 'npx', args: [] },
        },
      };

      const result = getInstalledExternalMCPs(config, registry);

      expect(result.length).toBe(1);
      expect(result[0].id).toBe('mcp-1');
    });

    it('should return empty array when registry is empty', async () => {
      const { getInstalledExternalMCPs } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          'mcp-1': { command: 'npx', args: [] },
        },
      };

      const result = getInstalledExternalMCPs(config, []);

      expect(result).toEqual([]);
    });
  });

  describe('validateRequiredEnvVars', () => {
    const entryWithRequiredVars: MCPRegistryEntry = {
      id: 'test-mcp',
      name: 'Test MCP',
      description: 'A test MCP',
      category: 'developer-tools',
      repository: 'https://github.com/test/test-mcp',
      installationType: 'npx',
      installConfig: { command: 'npx', args: [] },
      requiredEnvVars: [
        { name: 'API_KEY', description: 'API Key' },
        { name: 'SECRET', description: 'Secret' },
        { name: 'ENDPOINT', description: 'Endpoint' },
      ],
    };

    it('should return valid when all required env vars are provided', async () => {
      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithRequiredVars, {
        API_KEY: 'key123',
        SECRET: 'secret456',
        ENDPOINT: 'https://api.example.com',
      });

      expect(result.valid).toBe(true);
      expect(result.missing).toEqual([]);
    });

    it('should return invalid with missing vars when some are missing', async () => {
      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithRequiredVars, {
        API_KEY: 'key123',
      });

      expect(result.valid).toBe(false);
      expect(result.missing).toContain('SECRET');
      expect(result.missing).toContain('ENDPOINT');
      expect(result.missing).not.toContain('API_KEY');
    });

    it('should return invalid when all required vars are missing', async () => {
      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithRequiredVars, {});

      expect(result.valid).toBe(false);
      expect(result.missing.length).toBe(3);
    });

    it('should return valid when entry has no required env vars', async () => {
      const entryWithoutRequiredVars: MCPRegistryEntry = {
        id: 'simple-mcp',
        name: 'Simple MCP',
        description: 'A simple MCP',
        category: 'developer-tools',
        repository: 'https://github.com/test/simple-mcp',
        installationType: 'npx',
        installConfig: { command: 'npx', args: [] },
      };

      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithoutRequiredVars, {});

      expect(result.valid).toBe(true);
      expect(result.missing).toEqual([]);
    });

    it('should return valid when entry has empty requiredEnvVars array', async () => {
      const entryWithEmptyRequiredVars: MCPRegistryEntry = {
        id: 'simple-mcp',
        name: 'Simple MCP',
        description: 'A simple MCP',
        category: 'developer-tools',
        repository: 'https://github.com/test/simple-mcp',
        installationType: 'npx',
        installConfig: { command: 'npx', args: [] },
        requiredEnvVars: [],
      };

      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithEmptyRequiredVars, {});

      expect(result.valid).toBe(true);
      expect(result.missing).toEqual([]);
    });

    it('should treat empty string values as missing', async () => {
      const { validateRequiredEnvVars } =
        await import('../../src/utils/mcp-config.js');

      const result = validateRequiredEnvVars(entryWithRequiredVars, {
        API_KEY: 'valid-key',
        SECRET: '',
        ENDPOINT: '',
      });

      expect(result.valid).toBe(false);
      expect(result.missing).toContain('SECRET');
      expect(result.missing).toContain('ENDPOINT');
    });
  });

  describe('getClientInstallStatus with custom path', () => {
    it('should use custom path when provided', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/custom/path/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      });

      const { getClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getClientInstallStatus('cursor', '/custom/path');

      expect(getMCPConfigPath).toHaveBeenCalledWith('cursor', '/custom/path');
      expect(configFileExists).toHaveBeenCalledWith('cursor', '/custom/path');
      expect(result.configPath).toBe('/custom/path/config.json');
    });
  });

  describe('getAllClientInstallStatus comprehensive', () => {
    it('should check all 8 supported clients', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/config.json');
      vi.mocked(configFileExists).mockReturnValue(false);
      vi.mocked(readMCPConfig).mockReturnValue(null);

      const { getAllClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getAllClientInstallStatus();

      expect(result.length).toBe(11);
      const clients = result.map(s => s.client);
      expect(clients).toContain('cursor');
      expect(clients).toContain('claude-desktop');
      expect(clients).toContain('claude-code');
      expect(clients).toContain('opencode');
      expect(clients).toContain('vscode-cline');
      expect(clients).toContain('vscode-roo');
      expect(clients).toContain('vscode-continue');
      expect(clients).toContain('windsurf');
      expect(clients).toContain('trae');
      expect(clients).toContain('antigravity');
      expect(clients).toContain('zed');
    });

    it('should correctly identify multiple installed clients', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);

      let callIndex = 0;
      vi.mocked(readMCPConfig).mockImplementation((): MCPConfig | null => {
        callIndex++;

        if (callIndex === 1 || callIndex === 3) {
          return { mcpServers: { octocode: { command: 'npx', args: [] } } };
        }
        return { mcpServers: {} };
      });

      const { getAllClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getAllClientInstallStatus();

      const installed = result.filter(s => s.octocodeInstalled);
      expect(installed.length).toBe(2);
    });
  });

  describe('getConfiguredMethod edge cases', () => {
    it('should return null for node command', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'node', args: ['index.js'] },
        },
      };

      expect(getConfiguredMethod(config)).toBeNull();
    });

    it('should return null for custom command', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: '/usr/local/bin/custom-mcp', args: [] },
        },
      };

      expect(getConfiguredMethod(config)).toBeNull();
    });

    it('should return null when mcpServers is null-ish', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      expect(getConfiguredMethod({} as MCPConfig)).toBeNull();
      expect(
        getConfiguredMethod({ mcpServers: undefined } as MCPConfig)
      ).toBeNull();
    });
  });

  describe('mergeOctocodeConfig Windows behavior', () => {
    it('should use Windows config when isWindows is true', async () => {
      vi.resetModules();

      // Per-test override (not hoisted) so the re-imported module sees Windows.
      // mcp-paths.js / mcp-io.js are already mocked at the top level, so we don't
      // re-`vi.mock` them here (that hoists and triggers a deprecation warning).
      vi.doMock('../../src/utils/platform.js', () => ({
        isWindows: true,
      }));

      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeOctocodeConfig({ mcpServers: {} }, 'npx');

      expect(result.mcpServers!.octocode.command).toBe('npx');
    });
  });
});
