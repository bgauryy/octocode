import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import {
  getConfig,
  getConfigSync,
  reloadConfig,
  resolveConfigSync,
  invalidateConfigCache,
  getConfigValue,
  _resetConfigCache,
  _getCacheState,
} from '../../src/config/resolver.js';
import { DEFAULT_CONFIG } from '../../src/config/defaults.js';

vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  readFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  writeFileSync: vi.fn(),
  unlinkSync: vi.fn(),
}));

describe('config/resolver', () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.clearAllMocks();
    _resetConfigCache();
    delete process.env.GITHUB_API_URL;
    delete process.env.ENABLE_LOCAL;
    delete process.env.ENABLE_CLONE;
    delete process.env.WORKSPACE_ROOT;
    delete process.env.ALLOWED_PATHS;
    delete process.env.TOOLS_TO_RUN;
    delete process.env.ENABLE_TOOLS;
    delete process.env.DISABLE_TOOLS;
    delete process.env.REQUEST_TIMEOUT;
    delete process.env.MAX_RETRIES;
    delete process.env.LOG;
    delete process.env.OCTOCODE_LSP_CONFIG;
    delete process.env.OCTOCODE_OUTPUT_FORMAT;
    delete process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    process.env = { ...originalEnv };
  });

  describe('resolveConfigSync', () => {
    it('returns defaults when no config file exists', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      const config = resolveConfigSync();

      expect(config.version).toBe(DEFAULT_CONFIG.version);
      expect(config.github.apiUrl).toBe(DEFAULT_CONFIG.github.apiUrl);
      expect(config.local.enabled).toBe(DEFAULT_CONFIG.local.enabled);
      expect(config.output.pagination.defaultCharLength).toBe(
        DEFAULT_CONFIG.output.pagination.defaultCharLength
      );
      expect(config.source).toBe('defaults');
    });

    it('loads config from file', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          version: 1,
          github: { apiUrl: 'https://github.example.com/api/v3' },
          local: { enabled: true },
        })
      );

      const config = resolveConfigSync();

      expect(config.github.apiUrl).toBe('https://github.example.com/api/v3');
      expect(config.local.enabled).toBe(true);
      expect(config.source).toBe('file');
    });

    it('env vars override file config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          github: { apiUrl: 'https://github.example.com/api/v3' },
          local: { enabled: false },
        })
      );

      process.env.GITHUB_API_URL = 'https://env.github.com/api/v3';
      process.env.ENABLE_LOCAL = 'true';
      process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '9000';

      const config = resolveConfigSync();

      expect(config.github.apiUrl).toBe('https://env.github.com/api/v3');
      expect(config.local.enabled).toBe(true);
      expect(config.output.pagination.defaultCharLength).toBe(9000);
      expect(config.source).toBe('mixed');
    });

    it('loads output pagination defaults from file config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          output: { pagination: { defaultCharLength: 9000 } },
        })
      );

      const config = resolveConfigSync();

      expect(config.output.pagination.defaultCharLength).toBe(9000);
      expect(config.source).toBe('file');
    });

    it('env vars override defaults when no file', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      process.env.ENABLE_LOCAL = '1';
      process.env.REQUEST_TIMEOUT = '60000';

      const config = resolveConfigSync();

      expect(config.local.enabled).toBe(true);
      expect(config.network.timeout).toBe(60000);
    });

    describe('environment variable parsing', () => {
      beforeEach(() => {
        vi.mocked(existsSync).mockReturnValue(false);
      });

      it('parses GITHUB_API_URL', () => {
        process.env.GITHUB_API_URL = 'https://custom.github.com/api';
        const config = resolveConfigSync();
        expect(config.github.apiUrl).toBe('https://custom.github.com/api');
      });

      it('parses ENABLE_LOCAL as boolean', () => {
        process.env.ENABLE_LOCAL = 'true';
        expect(resolveConfigSync().local.enabled).toBe(true);

        _resetConfigCache();
        process.env.ENABLE_LOCAL = '1';
        expect(resolveConfigSync().local.enabled).toBe(true);

        _resetConfigCache();
        process.env.ENABLE_LOCAL = 'false';
        expect(resolveConfigSync().local.enabled).toBe(false);

        _resetConfigCache();
        process.env.ENABLE_LOCAL = '0';
        expect(resolveConfigSync().local.enabled).toBe(false);
      });

      it('parses ENABLE_CLONE as boolean', () => {
        process.env.ENABLE_CLONE = 'true';
        expect(resolveConfigSync().local.enableClone).toBe(true);

        _resetConfigCache();
        process.env.ENABLE_CLONE = '1';
        expect(resolveConfigSync().local.enableClone).toBe(true);

        _resetConfigCache();
        process.env.ENABLE_CLONE = 'false';
        expect(resolveConfigSync().local.enableClone).toBe(false);

        _resetConfigCache();
        process.env.ENABLE_CLONE = '0';
        expect(resolveConfigSync().local.enableClone).toBe(false);
      });

      it('parses TOOLS_TO_RUN as string array', () => {
        process.env.TOOLS_TO_RUN = 'githubSearchCode,packageSearch';
        const config = resolveConfigSync();
        expect(config.tools.enabled).toEqual([
          'githubSearchCode',
          'packageSearch',
        ]);
      });

      it('parses ENABLE_TOOLS as string array', () => {
        process.env.ENABLE_TOOLS = 'localSearchCode, localViewStructure';
        const config = resolveConfigSync();
        expect(config.tools.enableAdditional).toEqual([
          'localSearchCode',
          'localViewStructure',
        ]);
      });

      it('parses DISABLE_TOOLS as string array', () => {
        process.env.DISABLE_TOOLS = 'packageSearch';
        const config = resolveConfigSync();
        expect(config.tools.disabled).toEqual(['packageSearch']);
      });

      it('parses REQUEST_TIMEOUT as number', () => {
        process.env.REQUEST_TIMEOUT = '45000';
        const config = resolveConfigSync();
        expect(config.network.timeout).toBe(45000);
      });

      it('clamps timeout to valid range', () => {
        process.env.REQUEST_TIMEOUT = '1000';
        expect(resolveConfigSync().network.timeout).toBe(5000);

        _resetConfigCache();
        process.env.REQUEST_TIMEOUT = '999999';
        expect(resolveConfigSync().network.timeout).toBe(300000);
      });

      it('clamps REQUEST_TIMEOUT=0 to MIN_TIMEOUT (5000)', () => {
        process.env.REQUEST_TIMEOUT = '0';
        expect(resolveConfigSync().network.timeout).toBe(5000);
      });

      it('parses MAX_RETRIES as number', () => {
        process.env.MAX_RETRIES = '5';
        const config = resolveConfigSync();
        expect(config.network.maxRetries).toBe(5);
      });

      it('clamps maxRetries to valid range', () => {
        process.env.MAX_RETRIES = '-1';
        expect(resolveConfigSync().network.maxRetries).toBe(0);

        _resetConfigCache();
        process.env.MAX_RETRIES = '99';
        expect(resolveConfigSync().network.maxRetries).toBe(10);
      });

      it('allows MAX_RETRIES=0 (valid value, no retries)', () => {
        process.env.MAX_RETRIES = '0';
        expect(resolveConfigSync().network.maxRetries).toBe(0);
      });

      it('parses LOG=false as false', () => {
        process.env.LOG = 'false';
        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(false);
      });

      it('parses LOG=yes as true (default-to-true semantics)', () => {
        process.env.LOG = 'yes';
        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(true);
      });

      it('parses LOG=anything as true (default-to-true semantics)', () => {
        process.env.LOG = 'anything';
        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(true);
      });

      it('parses OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH', () => {
        process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '12000';
        const config = resolveConfigSync();
        expect(config.output.pagination.defaultCharLength).toBe(12000);
      });

      it('clamps OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH to valid range', () => {
        process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '10';
        expect(resolveConfigSync().output.pagination.defaultCharLength).toBe(
          1000
        );

        _resetConfigCache();
        process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '999999';
        expect(resolveConfigSync().output.pagination.defaultCharLength).toBe(
          50000
        );
      });

      it('parses ALLOWED_PATHS as comma-separated array', () => {
        process.env.ALLOWED_PATHS = '/path/a,/path/b,/path/c';
        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual([
          '/path/a',
          '/path/b',
          '/path/c',
        ]);
      });

      it('trims whitespace from ALLOWED_PATHS entries', () => {
        process.env.ALLOWED_PATHS = ' /path/a , /path/b ';
        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual(['/path/a', '/path/b']);
      });

      it('filters empty entries from ALLOWED_PATHS', () => {
        process.env.ALLOWED_PATHS = '/path/a,,/path/b,';
        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual(['/path/a', '/path/b']);
      });

      it('parses OCTOCODE_LSP_CONFIG', () => {
        process.env.OCTOCODE_LSP_CONFIG = '/custom/lsp-config.json';
        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe('/custom/lsp-config.json');
      });

      it('trims whitespace from OCTOCODE_LSP_CONFIG', () => {
        process.env.OCTOCODE_LSP_CONFIG = '  /custom/lsp-config.json  ';
        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe('/custom/lsp-config.json');
      });

      it('ignores empty OCTOCODE_LSP_CONFIG', () => {
        process.env.OCTOCODE_LSP_CONFIG = '   ';
        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe(DEFAULT_CONFIG.lsp.configPath);
      });

      it('ignores invalid boolean env vars and falls back to default', () => {
        process.env.ENABLE_LOCAL = 'notabool';
        const config = resolveConfigSync();
        expect(config.local.enabled).toBe(DEFAULT_CONFIG.local.enabled);
      });

      it('ignores invalid number env vars and falls back to default', () => {
        process.env.REQUEST_TIMEOUT = 'notanumber';
        const config = resolveConfigSync();
        expect(config.network.timeout).toBe(DEFAULT_CONFIG.network.timeout);
      });

      it('ignores empty string array env vars', () => {
        process.env.TOOLS_TO_RUN = '';
        const config = resolveConfigSync();
        expect(config.tools.enabled).toBe(DEFAULT_CONFIG.tools.enabled);
      });
    });
  });

  describe('fallback chain: env → file → default', () => {
    describe('github.apiUrl', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            github: { apiUrl: 'https://file.github.com/api/v3' },
          })
        );
        process.env.GITHUB_API_URL = 'https://env.github.com/api/v3';

        const config = resolveConfigSync();
        expect(config.github.apiUrl).toBe('https://env.github.com/api/v3');
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            github: { apiUrl: 'https://file.github.com/api/v3' },
          })
        );

        const config = resolveConfigSync();
        expect(config.github.apiUrl).toBe('https://file.github.com/api/v3');
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.github.apiUrl).toBe(DEFAULT_CONFIG.github.apiUrl);
      });
    });

    describe('local.enabled', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ local: { enabled: true } })
        );
        process.env.ENABLE_LOCAL = 'false';

        const config = resolveConfigSync();
        expect(config.local.enabled).toBe(false);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ local: { enabled: false } })
        );

        const config = resolveConfigSync();
        expect(config.local.enabled).toBe(false);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.local.enabled).toBe(DEFAULT_CONFIG.local.enabled);
      });
    });

    describe('local.enableClone', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ local: { enableClone: true } })
        );
        process.env.ENABLE_CLONE = 'false';

        const config = resolveConfigSync();
        expect(config.local.enableClone).toBe(false);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ local: { enableClone: true } })
        );

        const config = resolveConfigSync();
        expect(config.local.enableClone).toBe(true);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.local.enableClone).toBe(DEFAULT_CONFIG.local.enableClone);
      });
    });

    describe('local.allowedPaths', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            local: { allowedPaths: ['/file/path'] },
          })
        );
        process.env.ALLOWED_PATHS = '/env/path1,/env/path2';

        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual(['/env/path1', '/env/path2']);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            local: { allowedPaths: ['/file/path1', '/file/path2'] },
          })
        );

        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual([
          '/file/path1',
          '/file/path2',
        ]);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.local.allowedPaths).toEqual(
          DEFAULT_CONFIG.local.allowedPaths
        );
      });
    });

    describe('tools.enabled (TOOLS_TO_RUN)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { enabled: ['fileTool'] },
          })
        );
        process.env.TOOLS_TO_RUN = 'envTool1,envTool2';

        const config = resolveConfigSync();
        expect(config.tools.enabled).toEqual(['envTool1', 'envTool2']);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { enabled: ['fileTool'] },
          })
        );

        const config = resolveConfigSync();
        expect(config.tools.enabled).toEqual(['fileTool']);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.tools.enabled).toBe(DEFAULT_CONFIG.tools.enabled);
      });
    });

    describe('tools.enableAdditional (ENABLE_TOOLS)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { enableAdditional: ['fileTool'] },
          })
        );
        process.env.ENABLE_TOOLS = 'envTool';

        const config = resolveConfigSync();
        expect(config.tools.enableAdditional).toEqual(['envTool']);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { enableAdditional: ['fileTool'] },
          })
        );

        const config = resolveConfigSync();
        expect(config.tools.enableAdditional).toEqual(['fileTool']);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.tools.enableAdditional).toBe(
          DEFAULT_CONFIG.tools.enableAdditional
        );
      });
    });

    describe('tools.disabled (DISABLE_TOOLS)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { disabled: ['fileTool'] },
          })
        );
        process.env.DISABLE_TOOLS = 'envTool';

        const config = resolveConfigSync();
        expect(config.tools.disabled).toEqual(['envTool']);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            tools: { disabled: ['fileTool'] },
          })
        );

        const config = resolveConfigSync();
        expect(config.tools.disabled).toEqual(['fileTool']);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.tools.disabled).toBe(DEFAULT_CONFIG.tools.disabled);
      });
    });

    describe('network.timeout (REQUEST_TIMEOUT)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ network: { timeout: 20000 } })
        );
        process.env.REQUEST_TIMEOUT = '60000';

        const config = resolveConfigSync();
        expect(config.network.timeout).toBe(60000);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ network: { timeout: 15000 } })
        );

        const config = resolveConfigSync();
        expect(config.network.timeout).toBe(15000);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.network.timeout).toBe(DEFAULT_CONFIG.network.timeout);
      });
    });

    describe('network.maxRetries (MAX_RETRIES)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ network: { maxRetries: 2 } })
        );
        process.env.MAX_RETRIES = '7';

        const config = resolveConfigSync();
        expect(config.network.maxRetries).toBe(7);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ network: { maxRetries: 5 } })
        );

        const config = resolveConfigSync();
        expect(config.network.maxRetries).toBe(5);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.network.maxRetries).toBe(
          DEFAULT_CONFIG.network.maxRetries
        );
      });
    });

    describe('telemetry.logging (LOG)', () => {
      it('LOG=false overrides file logging: true', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ telemetry: { logging: true } })
        );
        process.env.LOG = 'false';

        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(false);
      });

      it('LOG=yes overrides file logging: false (default-to-true semantics)', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ telemetry: { logging: false } })
        );
        process.env.LOG = 'yes';

        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(true);
      });

      it('LOG=anything overrides file logging: false', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ telemetry: { logging: false } })
        );
        process.env.LOG = 'enabled';

        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(true);
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ telemetry: { logging: false } })
        );

        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(false);
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.telemetry.logging).toBe(DEFAULT_CONFIG.telemetry.logging);
      });
    });

    describe('lsp.configPath (OCTOCODE_LSP_CONFIG)', () => {
      it('env overrides file', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            lsp: { configPath: '/file/lsp-config.json' },
          })
        );
        process.env.OCTOCODE_LSP_CONFIG = '/env/lsp-config.json';

        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe('/env/lsp-config.json');
      });

      it('file overrides default', () => {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({
            lsp: { configPath: '/file/lsp-config.json' },
          })
        );

        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe('/file/lsp-config.json');
      });

      it('falls back to default when neither env nor file', () => {
        vi.mocked(existsSync).mockReturnValue(false);

        const config = resolveConfigSync();
        expect(config.lsp.configPath).toBe(DEFAULT_CONFIG.lsp.configPath);
      });
    });
  });

  describe('source detection', () => {
    it('source is "defaults" when no file and no env', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      const config = resolveConfigSync();
      expect(config.source).toBe('defaults');
    });

    it('source is "file" when file exists and no env overrides', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({ version: 1 }));

      const config = resolveConfigSync();
      expect(config.source).toBe('file');
    });

    it('source is "mixed" when file exists and env overrides are set', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({ version: 1 }));
      process.env.LOG = 'false';

      const config = resolveConfigSync();
      expect(config.source).toBe('mixed');
    });

    it('source is "defaults" when no file even with env overrides', () => {
      vi.mocked(existsSync).mockReturnValue(false);
      process.env.ALLOWED_PATHS = '/some/path';

      const config = resolveConfigSync();
      expect(config.source).toBe('defaults');
    });

    it('detects ALLOWED_PATHS as env override for mixed source', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({ version: 1 }));
      process.env.ALLOWED_PATHS = '/path/a';

      const config = resolveConfigSync();
      expect(config.source).toBe('mixed');
    });

    it('detects OCTOCODE_LSP_CONFIG as env override for mixed source', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({ version: 1 }));
      process.env.OCTOCODE_LSP_CONFIG = '/path/to/config.json';

      const config = resolveConfigSync();
      expect(config.source).toBe('mixed');
    });
  });

  describe('getConfigSync', () => {
    it('returns cached config on subsequent calls', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      const config1 = getConfigSync();
      const callsAfterFirst = vi.mocked(existsSync).mock.calls.length;

      const config2 = getConfigSync();
      const callsAfterSecond = vi.mocked(existsSync).mock.calls.length;

      expect(config1).toBe(config2);
      expect(callsAfterSecond).toBe(callsAfterFirst);
    });

    it('respects cache TTL', async () => {
      vi.mocked(existsSync).mockReturnValue(false);

      getConfigSync();
      expect(_getCacheState().cached).toBe(true);
    });

    it('expires cache after TTL and reloads fresh config', () => {
      vi.useFakeTimers();

      try {
        vi.mocked(existsSync).mockReturnValue(true);
        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ version: 1, local: { enabled: false } })
        );

        const config1 = getConfigSync();
        expect(config1.local.enabled).toBe(false);
        const callsAfterFirst = vi.mocked(existsSync).mock.calls.length;

        const config2 = getConfigSync();
        expect(config2).toBe(config1);
        expect(vi.mocked(existsSync).mock.calls.length).toBe(callsAfterFirst);

        vi.mocked(readFileSync).mockReturnValue(
          JSON.stringify({ version: 1, local: { enabled: true } })
        );

        vi.advanceTimersByTime(61000);

        const config3 = getConfigSync();
        expect(config3).not.toBe(config1);
        expect(config3.local.enabled).toBe(true);
        expect(vi.mocked(existsSync).mock.calls.length).toBeGreaterThan(
          callsAfterFirst
        );
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('getConfig', () => {
    it('returns same result as getConfigSync', async () => {
      vi.mocked(existsSync).mockReturnValue(false);

      const asyncConfig = await getConfig();
      _resetConfigCache();
      const syncConfig = getConfigSync();

      expect(asyncConfig.version).toBe(syncConfig.version);
      expect(asyncConfig.github.apiUrl).toBe(syncConfig.github.apiUrl);
    });
  });

  describe('reloadConfig', () => {
    it('invalidates cache and reloads', async () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        '{"version": 1, "local": {"enabled": false}}'
      );

      const config1 = getConfigSync();
      expect(config1.local.enabled).toBe(false);

      vi.mocked(readFileSync).mockReturnValue(
        '{"version": 1, "local": {"enabled": true}}'
      );

      const config2 = getConfigSync();
      expect(config2.local.enabled).toBe(false);

      const config3 = await reloadConfig();
      expect(config3.local.enabled).toBe(true);
    });
  });

  describe('invalidateConfigCache', () => {
    it('clears the cache', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      getConfigSync();
      expect(_getCacheState().cached).toBe(true);

      invalidateConfigCache();
      expect(_getCacheState().cached).toBe(false);
    });
  });

  describe('getConfigValue', () => {
    beforeEach(() => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          version: 1,
          github: { apiUrl: 'https://api.github.com' },
          local: { enabled: true },
        })
      );
    });

    it('gets top-level value', () => {
      expect(getConfigValue('version')).toBe(1);
    });

    it('gets nested value', () => {
      expect(getConfigValue('github.apiUrl')).toBe('https://api.github.com');
      expect(getConfigValue('local.enabled')).toBe(true);
    });

    it('returns undefined for non-existent path', () => {
      expect(getConfigValue('nonexistent')).toBeUndefined();
      expect(getConfigValue('github.nonexistent')).toBeUndefined();
      expect(getConfigValue('a.b.c.d')).toBeUndefined();
    });
  });

  describe('file config with defaults', () => {
    it('merges file config with defaults', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          github: { apiUrl: 'https://custom.github.com/api/v3' },
        })
      );

      const config = resolveConfigSync();

      expect(config.github.apiUrl).toBe('https://custom.github.com/api/v3');
      expect(config.local.enabled).toBe(DEFAULT_CONFIG.local.enabled);
    });
  });

  describe('empty config {} uses all defaults', () => {
    it('empty object {} resolves every field to its default', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({}));

      const config = resolveConfigSync();

      expect(config.version).toBe(DEFAULT_CONFIG.version);
      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.local).toEqual(DEFAULT_CONFIG.local);
      expect(config.tools).toEqual(DEFAULT_CONFIG.tools);
      expect(config.network).toEqual(DEFAULT_CONFIG.network);
      expect(config.telemetry).toEqual(DEFAULT_CONFIG.telemetry);
      expect(config.lsp).toEqual(DEFAULT_CONFIG.lsp);
      expect(config.output).toEqual(DEFAULT_CONFIG.output);
    });

    it('empty file content resolves every field to its default', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('');

      const config = resolveConfigSync();

      expect(config.version).toBe(DEFAULT_CONFIG.version);
      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.local).toEqual(DEFAULT_CONFIG.local);
      expect(config.tools).toEqual(DEFAULT_CONFIG.tools);
      expect(config.network).toEqual(DEFAULT_CONFIG.network);
      expect(config.telemetry).toEqual(DEFAULT_CONFIG.telemetry);
      expect(config.lsp).toEqual(DEFAULT_CONFIG.lsp);
      expect(config.output).toEqual(DEFAULT_CONFIG.output);
    });

    it('whitespace-only file content resolves to all defaults', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('   \n\t  \n  ');

      const config = resolveConfigSync();

      expect(config.version).toBe(DEFAULT_CONFIG.version);
      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.network).toEqual(DEFAULT_CONFIG.network);
    });

    it('config with all empty section objects resolves to defaults', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          github: {},
          local: {},
          tools: {},
          network: {},
          telemetry: {},
          lsp: {},
          output: {},
        })
      );

      const config = resolveConfigSync();

      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.local).toEqual(DEFAULT_CONFIG.local);
      expect(config.tools).toEqual(DEFAULT_CONFIG.tools);
      expect(config.network).toEqual(DEFAULT_CONFIG.network);
      expect(config.telemetry).toEqual(DEFAULT_CONFIG.telemetry);
      expect(config.lsp).toEqual(DEFAULT_CONFIG.lsp);
      expect(config.output).toEqual(DEFAULT_CONFIG.output);
    });

    it('config with single empty section — other sections still default', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({ network: {} }));

      const config = resolveConfigSync();

      expect(config.network).toEqual(DEFAULT_CONFIG.network);
      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.local).toEqual(DEFAULT_CONFIG.local);
    });

    it('empty config {} source is "file" (file exists, just empty)', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({}));

      const config = resolveConfigSync();
      expect(config.source).toBe('file');
      expect(config.configPath).toBeDefined();
    });

    it('env overrides still apply even when config is empty {}', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(JSON.stringify({}));
      process.env.ENABLE_LOCAL = 'true';
      process.env.REQUEST_TIMEOUT = '60000';

      const config = resolveConfigSync();

      expect(config.local.enabled).toBe(true);
      expect(config.network.timeout).toBe(60000);
      expect(config.github).toEqual(DEFAULT_CONFIG.github);
      expect(config.source).toBe('mixed');
    });

    it('partial section with one value — other fields in same section default', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          local: { enabled: true },
        })
      );

      const config = resolveConfigSync();

      expect(config.local.enabled).toBe(true);
      expect(config.local.enableClone).toBe(DEFAULT_CONFIG.local.enableClone);
      expect(config.local.allowedPaths).toEqual(
        DEFAULT_CONFIG.local.allowedPaths
      );
    });
  });

  describe('error handling', () => {
    it('falls back to defaults on parse error', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{ invalid json }');

      const config = resolveConfigSync();
      expect(config.source).toBe('defaults');
      expect(config.github.apiUrl).toBe(DEFAULT_CONFIG.github.apiUrl);
    });

    it('falls back to defaults when config has validation errors', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          version: 1,
          github: { apiUrl: 'not-a-valid-url' },
          local: { enabled: true },
        })
      );

      const config = resolveConfigSync();

      expect(config.source).toBe('defaults');
      expect(config.github.apiUrl).toBe(DEFAULT_CONFIG.github.apiUrl);
      expect(config.local.enabled).toBe(DEFAULT_CONFIG.local.enabled);
    });

    it('loads config normally when validation has only warnings', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          version: 1,
          local: { enabled: false },
          unknownKey: 'triggers warning but not error',
        })
      );

      const config = resolveConfigSync();

      expect(config.source).toBe('file');
      expect(config.local.enabled).toBe(false);
    });

    it('env overrides still apply when invalid config falls back to defaults', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        JSON.stringify({
          version: 1,
          network: { timeout: -999 },
        })
      );
      process.env.ENABLE_LOCAL = 'false';

      const config = resolveConfigSync();

      expect(config.local.enabled).toBe(false);
      expect(config.network.timeout).toBe(DEFAULT_CONFIG.network.timeout);
    });
  });
});
