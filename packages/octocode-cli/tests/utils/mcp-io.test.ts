import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { MCPConfig } from '../../src/types/index.js';

vi.mock('../../src/utils/fs.js', () => ({
  fileExists: vi.fn(),
  readJsonFile: vi.fn(),
  writeJsonFile: vi.fn(),
  backupFile: vi.fn(),
  dirExists: vi.fn(),
}));

vi.mock('node:fs', () => ({
  default: {
    mkdirSync: vi.fn(),
  },
  mkdirSync: vi.fn(),
}));

describe('MCP I/O Utilities', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('readMCPConfig', () => {
    it('should return empty config when file does not exist', async () => {
      const { fileExists } = await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(false);

      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');
      const result = readMCPConfig('/path/to/config.json');

      expect(result).toEqual({ mcpServers: {} });
    });

    it('should return parsed config when file exists', async () => {
      const { fileExists, readJsonFile } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readJsonFile).mockReturnValue({
        mcpServers: { test: { command: 'node', args: [] } },
      });

      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');
      const result = readMCPConfig('/path/to/config.json');

      expect(result).toEqual({
        mcpServers: { test: { command: 'node', args: [] } },
      });
    });

    it('should return null when JSON parsing fails', async () => {
      const { fileExists, readJsonFile } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(readJsonFile).mockReturnValue(null);

      const { readMCPConfig } = await import('../../src/utils/mcp-io.js');
      const result = readMCPConfig('/path/to/config.json');

      expect(result).toBeNull();
    });
  });

  describe('writeMCPConfig', () => {
    it('should write config successfully', async () => {
      const { fileExists, writeJsonFile, dirExists } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(false);
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(writeJsonFile).mockReturnValue(true);

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = {
        mcpServers: { test: { command: 'node', args: [] } },
      };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(true);
      expect(writeJsonFile).toHaveBeenCalled();
    });

    it('should create backup when file exists', async () => {
      const { fileExists, writeJsonFile, backupFile, dirExists } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(writeJsonFile).mockReturnValue(true);
      vi.mocked(backupFile).mockReturnValue('/path/to/config.json.backup');

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = {
        mcpServers: { test: { command: 'node', args: [] } },
      };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(true);
      expect(result.backupPath).toBe('/path/to/config.json.backup');
      expect(backupFile).toHaveBeenCalled();
    });

    it('should not create backup when createBackup is false', async () => {
      const { fileExists, writeJsonFile, backupFile, dirExists } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(true);
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(writeJsonFile).mockReturnValue(true);

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = {
        mcpServers: { test: { command: 'node', args: [] } },
      };
      const result = writeMCPConfig('/path/to/config.json', config, false);

      expect(result.success).toBe(true);
      expect(backupFile).not.toHaveBeenCalled();
    });

    it('should create directory if it does not exist', async () => {
      const fs = await import('node:fs');
      const { fileExists, writeJsonFile, dirExists } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(false);
      vi.mocked(dirExists).mockReturnValue(false);
      vi.mocked(writeJsonFile).mockReturnValue(true);
      vi.mocked(fs.default.mkdirSync).mockImplementation(() => '');

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = { mcpServers: {} };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(true);
      expect(fs.default.mkdirSync).toHaveBeenCalledWith('/path/to', {
        recursive: true,
        mode: 0o700,
      });
    });

    it('should return error when write fails', async () => {
      const { fileExists, writeJsonFile, dirExists } =
        await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockReturnValue(false);
      vi.mocked(dirExists).mockReturnValue(true);
      vi.mocked(writeJsonFile).mockReturnValue(false);

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = { mcpServers: {} };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Failed to write config file');
    });

    it('should return error on exception', async () => {
      const { fileExists } = await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockImplementation(() => {
        throw new Error('Permission denied');
      });

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = { mcpServers: {} };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Permission denied');
    });

    it('should handle unknown error type', async () => {
      const { fileExists } = await import('../../src/utils/fs.js');
      vi.mocked(fileExists).mockImplementation(() => {
        throw 'string error';
      });

      const { writeMCPConfig } = await import('../../src/utils/mcp-io.js');
      const config: MCPConfig = { mcpServers: {} };
      const result = writeMCPConfig('/path/to/config.json', config);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Unknown error');
    });
  });
});
