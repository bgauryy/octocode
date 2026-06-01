import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/utils/mcp-config.js', () => ({
  getMCPConfigPath: vi.fn(),
  readMCPConfig: vi.fn(),
  writeMCPConfig: vi.fn(),
  mergeOctocodeConfig: vi.fn(),
  isOctocodeConfigured: vi.fn(),
  clientConfigExists: vi.fn(),
  getOctocodeServerConfig: vi.fn(),
  getOctocodeServerConfigWindows: vi.fn(),
  getConfiguredMethod: vi.fn(),
}));

vi.mock('../../src/utils/fs.js', () => ({
  fileExists: vi.fn(),
}));

vi.mock('../../src/utils/platform.js', () => ({
  isWindows: false,
}));

describe('Install Feature', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('detectAvailableIDEs', () => {
    it('should return empty array when no IDEs are available', async () => {
      const { clientConfigExists } =
        await import('../../src/utils/mcp-config.js');
      vi.mocked(clientConfigExists).mockReturnValue(false);

      const { detectAvailableIDEs } =
        await import('../../src/features/install.js');
      const result = detectAvailableIDEs();

      expect(result).toEqual([]);
    });

    it('should return cursor when cursor is available', async () => {
      const { clientConfigExists } =
        await import('../../src/utils/mcp-config.js');
      vi.mocked(clientConfigExists).mockImplementation(
        client => client === 'cursor'
      );

      const { detectAvailableIDEs } =
        await import('../../src/features/install.js');
      const result = detectAvailableIDEs();

      expect(result).toContain('cursor');
      expect(result).not.toContain('claude');
    });

    it('should return both when both are available', async () => {
      const { clientConfigExists } =
        await import('../../src/utils/mcp-config.js');
      vi.mocked(clientConfigExists).mockReturnValue(true);

      const { detectAvailableIDEs } =
        await import('../../src/features/install.js');
      const result = detectAvailableIDEs();

      expect(result).toContain('cursor');
      expect(result).toContain('claude');
    });
  });

  describe('checkExistingInstallation', () => {
    it('should return not installed when config does not exist', async () => {
      const { getMCPConfigPath } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(false);

      const { checkExistingInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingInstallation('cursor');

      expect(result.installed).toBe(false);
      expect(result.configExists).toBe(false);
      expect(result.configPath).toBe('/path/to/config.json');
    });

    it('should return not installed when config exists but is invalid', async () => {
      const { getMCPConfigPath, readMCPConfig } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue(null);

      const { checkExistingInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingInstallation('cursor');

      expect(result.installed).toBe(false);
      expect(result.configExists).toBe(true);
    });

    it('should return installed when octocode is configured', async () => {
      const { getMCPConfigPath, readMCPConfig, isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);

      const { checkExistingInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingInstallation('cursor');

      expect(result.installed).toBe(true);
      expect(result.configExists).toBe(true);
    });
  });

  describe('installOctocode', () => {
    it('should fail if already installed without force', async () => {
      const { getMCPConfigPath, readMCPConfig, isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);

      const { installOctocode } = await import('../../src/features/install.js');
      const result = installOctocode({
        ide: 'cursor',
        method: 'npx',
        force: false,
      });

      expect(result.success).toBe(false);
      expect(result.alreadyInstalled).toBe(true);
      expect(result.error).toContain('already configured');
    });

    it('should succeed when not already installed', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

      const { installOctocode } = await import('../../src/features/install.js');
      const result = installOctocode({
        ide: 'cursor',
        method: 'npx',
      });

      expect(result.success).toBe(true);
      expect(result.configPath).toBe('/path/to/config.json');
    });

    it('should succeed with force even when already installed', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'old', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({
        success: true,
        backupPath: '/path/to/backup',
      });

      const { installOctocode } = await import('../../src/features/install.js');
      const result = installOctocode({
        ide: 'cursor',
        method: 'npx',
        force: true,
      });

      expect(result.success).toBe(true);
      expect(result.backupPath).toBe('/path/to/backup');
    });

    it('should return error when write fails', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({
        success: false,
        error: 'Permission denied',
      });

      const { installOctocode } = await import('../../src/features/install.js');
      const result = installOctocode({
        ide: 'cursor',
        method: 'npx',
      });

      expect(result.success).toBe(false);
      expect(result.error).toBe('Permission denied');
    });

    it('should create new config when none exists', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue(null);
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

      const { installOctocode } = await import('../../src/features/install.js');
      const result = installOctocode({
        ide: 'cursor',
        method: 'npx',
      });

      expect(result.success).toBe(true);
      expect(mergeOctocodeConfig).toHaveBeenCalledWith(
        { mcpServers: {} },
        'npx'
      );
    });
  });

  describe('installOctocodeMultiple', () => {
    it('should install for multiple IDEs', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

      const { installOctocodeMultiple } =
        await import('../../src/features/install.js');
      const results = installOctocodeMultiple(['cursor', 'claude'], 'npx');

      expect(results.size).toBe(2);
      expect(results.get('cursor')?.success).toBe(true);
      expect(results.get('claude')?.success).toBe(true);
    });

    it('should handle mixed success/failure', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      let callCount = 0;
      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockImplementation(() => {
        callCount++;
        return callCount === 1
          ? { success: true }
          : { success: false, error: 'Failed' };
      });

      const { installOctocodeMultiple } =
        await import('../../src/features/install.js');
      const results = installOctocodeMultiple(['cursor', 'claude'], 'npx');

      expect(results.get('cursor')?.success).toBe(true);
      expect(results.get('claude')?.success).toBe(false);
    });
  });

  describe('getInstallPreview', () => {
    it('should return create action for new config', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(false);
      vi.mocked(readMCPConfig).mockReturnValue(null);
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });

      const { getInstallPreview } =
        await import('../../src/features/install.js');
      const preview = getInstallPreview('cursor', 'npx');

      expect(preview.action).toBe('create');
      expect(preview.ide).toBe('cursor');
      expect(preview.method).toBe('npx');
    });

    it('should return add action when config exists but no octocode', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { other: { command: 'node', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });

      const { getInstallPreview } =
        await import('../../src/features/install.js');
      const preview = getInstallPreview('cursor', 'npx');

      expect(preview.action).toBe('add');
    });

    it('should return override action when octocode is already installed', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
        getConfiguredMethod,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });
      vi.mocked(getConfiguredMethod).mockReturnValue('npx');

      const { getInstallPreview } =
        await import('../../src/features/install.js');
      const preview = getInstallPreview('cursor', 'npx');

      expect(preview.action).toBe('override');
      expect(preview.existingMethod).toBe('npx');
    });
  });

  describe('detectAvailableClients', () => {
    it('should return available clients', async () => {
      const { clientConfigExists } =
        await import('../../src/utils/mcp-config.js');
      vi.mocked(clientConfigExists).mockImplementation(
        client => client === 'cursor' || client === 'claude-code'
      );

      const { detectAvailableClients } =
        await import('../../src/features/install.js');
      const result = detectAvailableClients();

      expect(result).toContain('cursor');
      expect(result).toContain('claude-code');
      expect(result).not.toContain('claude-desktop');
    });

    it('should return empty array when no clients are available', async () => {
      const { clientConfigExists } =
        await import('../../src/utils/mcp-config.js');
      vi.mocked(clientConfigExists).mockReturnValue(false);

      const { detectAvailableClients } =
        await import('../../src/features/install.js');
      const result = detectAvailableClients();

      expect(result).toEqual([]);
    });
  });

  describe('checkExistingClientInstallation', () => {
    it('should handle custom path for custom client', async () => {
      const { readMCPConfig, isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);

      const { checkExistingClientInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingClientInstallation(
        'custom',
        '/custom/path.json'
      );

      expect(result.installed).toBe(true);
      expect(result.configPath).toBe('/custom/path.json');
    });

    it('should use getMCPConfigPath for non-custom clients', async () => {
      const { getMCPConfigPath, readMCPConfig, isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/cursor.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);

      const { checkExistingClientInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingClientInstallation('cursor');

      expect(getMCPConfigPath).toHaveBeenCalledWith('cursor', undefined);
      expect(result.installed).toBe(true);
    });

    it('should report configExists when file exists but readMCPConfig returns null', async () => {
      const { getMCPConfigPath, readMCPConfig } =
        await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/corrupt.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue(null);

      const { checkExistingClientInstallation } =
        await import('../../src/features/install.js');
      const result = checkExistingClientInstallation('cursor');

      expect(result.configPath).toBe('/path/to/corrupt.json');
      expect(result.configExists).toBe(true);
      expect(result.installed).toBe(false);
    });
  });

  describe('installOctocodeForClient', () => {
    it('should install for a specific client', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

      const { installOctocodeForClient } =
        await import('../../src/features/install.js');
      const result = installOctocodeForClient({
        client: 'cursor',
        method: 'npx',
      });

      expect(result.success).toBe(true);
    });

    it('should use custom path for custom client', async () => {
      const {
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({ success: true });

      const { installOctocodeForClient } =
        await import('../../src/features/install.js');
      const result = installOctocodeForClient({
        client: 'custom',
        method: 'npx',
        customPath: '/custom/path.json',
      });

      expect(result.success).toBe(true);
      expect(result.configPath).toBe('/custom/path.json');
    });

    it('should fail when already installed without force', async () => {
      const { getMCPConfigPath, readMCPConfig, isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);

      const { installOctocodeForClient } =
        await import('../../src/features/install.js');
      const result = installOctocodeForClient({
        client: 'cursor',
        method: 'npx',
        force: false,
      });

      expect(result.success).toBe(false);
      expect(result.alreadyInstalled).toBe(true);
      expect(result.error).toContain('already configured');
    });

    it('should return writeMCPConfig error when write fails', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        mergeOctocodeConfig,
        writeMCPConfig,
      } = await import('../../src/utils/mcp-config.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(readMCPConfig).mockReturnValue({ mcpServers: {} });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(mergeOctocodeConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(writeMCPConfig).mockReturnValue({
        success: false,
        error: 'disk full',
      });

      const { installOctocodeForClient } =
        await import('../../src/features/install.js');
      const result = installOctocodeForClient({
        client: 'cursor',
        method: 'npx',
      });

      expect(result.success).toBe(false);
      expect(result.error).toBe('disk full');
    });
  });

  describe('getInstallPreviewForClient', () => {
    it('should return preview for client installation', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(false);
      vi.mocked(readMCPConfig).mockReturnValue(null);
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });

      const { getInstallPreviewForClient } =
        await import('../../src/features/install.js');
      const preview = getInstallPreviewForClient('cursor', 'npx');

      expect(preview.action).toBe('create');
      expect(preview.client).toBe('cursor');
      expect(preview.method).toBe('npx');
    });

    it('should return override action when octocode is already installed', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
        getConfiguredMethod,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { octocode: { command: 'npx', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(true);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });
      vi.mocked(getConfiguredMethod).mockReturnValue('npx');

      const { getInstallPreviewForClient } =
        await import('../../src/features/install.js');
      const preview = getInstallPreviewForClient('cursor', 'npx');

      expect(preview.action).toBe('override');
      expect(preview.existingMethod).toBe('npx');
    });

    it('should return add action when config exists but octocode is not installed', async () => {
      const {
        getMCPConfigPath,
        readMCPConfig,
        isOctocodeConfigured,
        getOctocodeServerConfig,
      } = await import('../../src/utils/mcp-config.js');
      const { fileExists } = await import('../../src/utils/fs.js');

      vi.mocked(getMCPConfigPath).mockReturnValue('/path/to/config.json');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readMCPConfig).mockReturnValue({
        mcpServers: { other: { command: 'node', args: [] } },
      });
      vi.mocked(isOctocodeConfigured).mockReturnValue(false);
      vi.mocked(getOctocodeServerConfig).mockReturnValue({
        command: 'npx',
        args: ['octocode-mcp@latest'],
      });

      const { getInstallPreviewForClient } =
        await import('../../src/features/install.js');
      const preview = getInstallPreviewForClient('cursor', 'npx');

      expect(preview.action).toBe('add');
    });
  });
});
