import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { MCPConfig } from '../../src/types/index.js';

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

describe('MCP Config Extended', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('getOctocodeServerConfig', () => {
    it('should return npx config', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfig('npx');

      expect(result.command).toBe('npx');
      expect(result.args).toContain('@octocodeai/mcp@latest');
    });

    it('should throw error for unknown method', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      expect(() => getOctocodeServerConfig('invalid' as any)).toThrow(
        'Unknown install method'
      );
    });
  });

  describe('getOctocodeServerConfigWindows', () => {
    it('should return npx config for npx method', async () => {
      const { getOctocodeServerConfigWindows } =
        await import('../../src/utils/mcp-config.js');

      const result = getOctocodeServerConfigWindows('npx');

      expect(result.command).toBe('npx');
      expect(result.args).toContain('@octocodeai/mcp@latest');
    });
  });

  describe('mergeOctocodeConfig', () => {
    it('should add octocode to empty config', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const result = mergeOctocodeConfig({ mcpServers: {} }, 'npx');

      expect(result.mcpServers!.octocode).toBeDefined();
      expect(result.mcpServers!.octocode.command).toBe('npx');
    });

    it('should preserve existing servers', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const existing: MCPConfig = {
        mcpServers: {
          other: { command: 'node', args: ['other.js'] },
        },
      };

      const result = mergeOctocodeConfig(existing, 'npx');

      expect(result.mcpServers!.other).toEqual({
        command: 'node',
        args: ['other.js'],
      });
      expect(result.mcpServers!.octocode).toBeDefined();
    });

    it('should overwrite existing octocode config', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');

      const existing: MCPConfig = {
        mcpServers: {
          octocode: { command: 'old', args: [] },
        },
      };

      const result = mergeOctocodeConfig(existing, 'npx');

      expect(result.mcpServers!.octocode.command).toBe('npx');
    });
  });

  describe('isOctocodeConfigured', () => {
    it('should return true when octocode is configured', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      };

      expect(isOctocodeConfigured(config)).toBe(true);
    });

    it('should return false when octocode is not configured', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = { mcpServers: {} };

      expect(isOctocodeConfigured(config)).toBe(false);
    });

    it('should return false when mcpServers is undefined', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');

      const config = {} as MCPConfig;

      expect(isOctocodeConfigured(config)).toBe(false);
    });
  });

  describe('getConfiguredMethod', () => {
    it('should return npx when command is npx', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      };

      expect(getConfiguredMethod(config)).toBe('npx');
    });

    it('should return null for legacy bash command', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'bash', args: [] },
        },
      };

      expect(getConfiguredMethod(config)).toBeNull();
    });

    it('should return null for legacy powershell command', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'powershell', args: [] },
        },
      };

      expect(getConfiguredMethod(config)).toBeNull();
    });

    it('should return null when no octocode config', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = { mcpServers: {} };

      expect(getConfiguredMethod(config)).toBeNull();
    });

    it('should return null for unknown command', async () => {
      const { getConfiguredMethod } =
        await import('../../src/utils/mcp-config.js');

      const config: MCPConfig = {
        mcpServers: {
          octocode: { command: 'unknown', args: [] },
        },
      };

      expect(getConfiguredMethod(config)).toBeNull();
    });
  });

  describe('getClientInstallStatus', () => {
    it('should return status for client with config', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: {
          octocode: { command: 'npx', args: [] },
        },
      });

      const { getClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getClientInstallStatus('cursor');

      expect(result.client).toBe('cursor');
      expect(result.configExists).toBe(true);
      expect(result.octocodeInstalled).toBe(true);
      expect(result.method).toBe('npx');
      expect(result.configPath).toBe('/path/to/config.json');
    });

    it('should return status for client without octocode', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });

      const { getClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getClientInstallStatus('cursor');

      expect(result.octocodeInstalled).toBe(false);
      expect(result.method).toBeNull();
    });

    it('should return status when config does not exist', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(false);

      const { getClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getClientInstallStatus('cursor');

      expect(result.configExists).toBe(false);
      expect(result.octocodeInstalled).toBe(false);
    });

    it('should handle null readMCPConfig result', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue(null);

      const { getClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getClientInstallStatus('cursor');

      expect(result.configExists).toBe(true);
      expect(result.octocodeInstalled).toBe(false);
    });
  });

  describe('getAllClientInstallStatus', () => {
    it('should return status for all clients', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(false);
      vi.mocked(readMCPConfig).mockReturnValue(null);

      const { getAllClientInstallStatus } =
        await import('../../src/utils/mcp-config.js');
      const result = getAllClientInstallStatus();

      expect(result.length).toBeGreaterThan(0);
      expect(result.some(s => s.client === 'cursor')).toBe(true);
      expect(result.some(s => s.client === 'claude-desktop')).toBe(true);
    });
  });

  describe('findInstalledClients', () => {
    it('should return only clients with octocode installed', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockImplementation(() => true);

      let callCount = 0;
      vi.mocked(readMCPConfig).mockImplementation((): MCPConfig => {
        callCount++;

        if (callCount === 1) {
          return { mcpServers: { octocode: { command: 'npx', args: [] } } };
        }
        return { mcpServers: {} };
      });

      const { findInstalledClients } =
        await import('../../src/utils/mcp-config.js');
      const result = findInstalledClients();

      expect(result.length).toBe(1);
      expect(result[0].octocodeInstalled).toBe(true);
    });

    it('should return empty array when no clients have octocode', async () => {
      const { getMCPConfigPath, configFileExists } =
        await import('../../src/utils/mcp-paths.js');
      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(configFileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });

      const { findInstalledClients } =
        await import('../../src/utils/mcp-config.js');
      const result = findInstalledClients();

      expect(result).toEqual([]);
    });
  });
});
