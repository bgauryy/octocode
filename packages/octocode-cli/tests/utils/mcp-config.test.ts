import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('octocode-shared', () => ({
  isWindows: false,
  isMac: true,
  isLinux: false,
  HOME: '/Users/test',
  getAppDataPath: vi.fn(() => '/Users/test'),
  getLocalAppDataPath: vi.fn(() => '/Users/test'),
  getPlatformName: vi.fn(() => 'macOS'),
  getArchitecture: vi.fn(() => 'arm64'),
}));

vi.mock('node:fs', () => ({
  default: {
    existsSync: vi.fn(),
    statSync: vi.fn(),
    readFileSync: vi.fn(),
    writeFileSync: vi.fn(),
    mkdirSync: vi.fn(),
    copyFileSync: vi.fn(),
  },
  existsSync: vi.fn(),
  statSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  copyFileSync: vi.fn(),
}));

describe('MCP Config Utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('getMCPConfigPath', () => {
    it('should return Cursor config path for macOS', async () => {
      const { getMCPConfigPath } =
        await import('../../src/utils/mcp-config.js');

      expect(getMCPConfigPath('cursor')).toBe('/Users/test/.cursor/mcp.json');
    });

    it('should return Claude Desktop config path for macOS', async () => {
      const { getMCPConfigPath } =
        await import('../../src/utils/mcp-config.js');

      expect(getMCPConfigPath('claude-desktop')).toBe(
        '/Users/test/Library/Application Support/Claude/claude_desktop_config.json'
      );
    });

    it('should return Claude Code config path', async () => {
      const { getMCPConfigPath } =
        await import('../../src/utils/mcp-config.js');

      expect(getMCPConfigPath('claude-code')).toBe('/Users/test/.claude.json');
    });

    it('should return Windsurf config path', async () => {
      const { getMCPConfigPath } =
        await import('../../src/utils/mcp-config.js');

      expect(getMCPConfigPath('windsurf')).toBe(
        '/Users/test/.codeium/windsurf/mcp_config.json'
      );
    });
  });

  describe('getOctocodeServerConfig', () => {
    it('should return npx config', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');
      const config = getOctocodeServerConfig('npx');

      expect(config.command).toBe('npx');
      expect(config.args).toContain('octocode-mcp@latest');
    });

    it('should throw for unknown method (direct removed)', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');

      expect(() => getOctocodeServerConfig('direct' as any)).toThrow(
        'Unknown install method'
      );
    });

    it('should add env options when provided', async () => {
      const { getOctocodeServerConfig } =
        await import('../../src/utils/mcp-config.js');
      const config = getOctocodeServerConfig('npx', {
        enableLocal: true,
        githubToken: 'test-token',
      });

      expect(config.env).toBeDefined();
      expect(config.env!.ENABLE_LOCAL).toBe('true');
      expect(config.env!.GITHUB_TOKEN).toBe('test-token');
    });
  });

  describe('mergeOctocodeConfig', () => {
    it('should add octocode to empty config', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');
      const result = mergeOctocodeConfig({ mcpServers: {} }, 'npx');

      expect(result.mcpServers?.octocode).toBeDefined();
      expect(result.mcpServers?.octocode.command).toBe('npx');
    });

    it('should preserve existing servers', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');
      const existing = {
        mcpServers: {
          other: { command: 'node', args: ['other.js'] },
        },
      };
      const result = mergeOctocodeConfig(existing, 'npx');

      expect(result.mcpServers?.other).toBeDefined();
      expect(result.mcpServers?.octocode).toBeDefined();
    });

    it('should overwrite existing octocode config', async () => {
      const { mergeOctocodeConfig } =
        await import('../../src/utils/mcp-config.js');
      const existing = {
        mcpServers: {
          octocode: { command: 'old', args: [] },
        },
      };
      const result = mergeOctocodeConfig(existing, 'npx');

      expect(result.mcpServers?.octocode.command).toBe('npx');
    });
  });

  describe('isOctocodeConfigured', () => {
    it('should return true if octocode is configured', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const config = {
        mcpServers: {
          octocode: { command: 'npx', args: ['octocode-mcp@latest'] },
        },
      };

      expect(isOctocodeConfigured(config)).toBe(true);
    });

    it('should return false if octocode is not configured', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const config = { mcpServers: {} };

      expect(isOctocodeConfigured(config)).toBe(false);
    });

    it('should return false if mcpServers is undefined', async () => {
      const { isOctocodeConfigured } =
        await import('../../src/utils/mcp-config.js');
      const config = {};

      expect(isOctocodeConfigured(config)).toBe(false);
    });
  });
});
