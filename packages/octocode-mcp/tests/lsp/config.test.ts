import { describe, it, expect, vi, beforeEach } from 'vitest';

const mockReadFile = vi.hoisted(() => vi.fn());
const mockGetConfigSync = vi.hoisted(() =>
  vi.fn(() => ({ lsp: { configPath: undefined } }))
);
const mockGetOctocodeDir = vi.hoisted(() =>
  vi.fn(() => '/home/user/.octocode')
);
const mockValidateLSPServerPath = vi.hoisted(() =>
  vi.fn(() => ({ isValid: true, resolvedPath: '/usr/bin/tls' }))
);

vi.mock('fs', () => ({
  promises: { readFile: mockReadFile },
}));

vi.mock('octocode-shared', () => ({
  getConfigSync: mockGetConfigSync,
  getOctocodeDir: mockGetOctocodeDir,
}));

vi.mock('../../src/lsp/validation.js', () => ({
  validateLSPServerPath: mockValidateLSPServerPath,
}));

import { loadUserConfig, resolveLanguageServer } from '../../src/lsp/config.js';

describe('lsp/config', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetOctocodeDir.mockReturnValue('/home/user/.octocode');
    mockGetConfigSync.mockReturnValue({ lsp: { configPath: undefined } });
  });

  describe('loadUserConfig — unsafe command filtering (line 49)', () => {
    it('filters out dangerous shell commands from user config', async () => {
      const configWithUnsafe = JSON.stringify({
        languageServers: {
          '.ts': {
            command: 'typescript-language-server',
            args: ['--stdio'],
            languageId: 'typescript',
          },
          '.sh': { command: 'bash', args: [], languageId: 'shellscript' },
        },
      });
      mockReadFile.mockResolvedValueOnce(configWithUnsafe);

      const result = await loadUserConfig('/workspace');
      expect(result['.ts']).toBeDefined();
      expect(result['.sh']).toBeUndefined();
    });
  });

  describe('loadUserConfig — getConfigSync throws (line 80)', () => {
    it('falls back gracefully when getConfigSync throws', async () => {
      mockGetConfigSync.mockImplementation(() => {
        throw new Error('not available');
      });
      mockReadFile.mockRejectedValue(new Error('ENOENT'));

      const result = await loadUserConfig();
      expect(result).toEqual({});
    });
  });

  describe('resolveLanguageServer — typescript-language-server branches', () => {
    it('returns default command when path validation fails (line 150)', () => {
      mockValidateLSPServerPath.mockReturnValue({ isValid: false });

      const config = {
        command: 'typescript-language-server',
        args: ['--stdio'],
        envVar: 'TS_LSP_PATH',
      };

      const result = resolveLanguageServer(config);
      expect(result.command).toBe('typescript-language-server');
    });
  });
});
