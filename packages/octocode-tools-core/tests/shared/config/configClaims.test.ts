/**
 * Validates every configuration claim documented in the repo README and
 * docs/CONFIGURATION.md against the actual resolver. This is the single
 * config pipeline that BOTH the MCP server and the CLI read (via getConfig /
 * resolveConfigSync), so validating it here validates both flows.
 *
 * Each test asserts the documented contract: env var -> `.octocoderc` key ->
 * default, in that precedence order, plus parsing and clamping behavior.
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  resolveGitHub,
  resolveLocal,
  resolveTools,
  resolveNetwork,
  resolveLsp,
  resolveOutput,
} from '../../../src/shared/config/resolverSections.js';
import {
  setRuntimeSurface,
  _resetRuntimeSurface,
} from '../../../src/shared/config/runtimeSurface.js';

const CONFIG_ENV_KEYS = [
  'GITHUB_API_URL',
  'ENABLE_LOCAL',
  'ENABLE_CLONE',
  'WORKSPACE_ROOT',
  'ALLOWED_PATHS',
  'TOOLS_TO_RUN',
  'ENABLE_TOOLS',
  'DISABLE_TOOLS',
  'REQUEST_TIMEOUT',
  'MAX_RETRIES',
  'OCTOCODE_LSP_CONFIG',
  'OCTOCODE_OUTPUT_FORMAT',
  'OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH',
];

describe('README/CONFIGURATION config claims', () => {
  const original: Record<string, string | undefined> = {};

  beforeEach(() => {
    _resetRuntimeSurface(); // default surface is 'mcp'
    for (const k of CONFIG_ENV_KEYS) {
      original[k] = process.env[k];
      delete process.env[k];
    }
  });

  afterEach(() => {
    _resetRuntimeSurface();
    for (const k of CONFIG_ENV_KEYS) {
      if (original[k] === undefined) delete process.env[k];
      else process.env[k] = original[k];
    }
  });

  describe('GITHUB_API_URL -> github.apiUrl (default https://api.github.com)', () => {
    it('defaults when neither env nor file set', () => {
      expect(resolveGitHub(undefined).apiUrl).toBe('https://api.github.com');
    });
    it('uses .octocoderc value when no env', () => {
      expect(resolveGitHub({ apiUrl: 'https://ghe/api/v3' }).apiUrl).toBe(
        'https://ghe/api/v3'
      );
    });
    it('env overrides .octocoderc (precedence)', () => {
      process.env.GITHUB_API_URL = 'https://env/api/v3';
      expect(resolveGitHub({ apiUrl: 'https://ghe/api/v3' }).apiUrl).toBe(
        'https://env/api/v3'
      );
    });
  });

  describe('ENABLE_LOCAL -> local.enabled (default true, explicit false disables)', () => {
    it('defaults to true when neither env nor file config disables it', () => {
      expect(resolveLocal(undefined).enabled).toBe(true);
    });

    it('ENABLE_LOCAL=false disables, overriding file=true', () => {
      process.env.ENABLE_LOCAL = 'false';
      expect(resolveLocal({ enabled: true }).enabled).toBe(false);
    });

    it('ENABLE_LOCAL=1 enables', () => {
      process.env.ENABLE_LOCAL = '1';
      expect(resolveLocal({ enabled: false }).enabled).toBe(true);
    });

    it('file value applies when no env', () => {
      expect(resolveLocal({ enabled: false }).enabled).toBe(false);
    });
  });

  describe('ENABLE_CLONE -> local.enableClone (default false)', () => {
    it('defaults to false', () => {
      expect(resolveLocal(undefined).enableClone).toBe(false);
    });
    it('ENABLE_CLONE=true enables, overriding file=false', () => {
      process.env.ENABLE_CLONE = 'true';
      expect(resolveLocal({ enableClone: false }).enableClone).toBe(true);
    });
  });

  describe('WORKSPACE_ROOT -> local.workspaceRoot (default undefined)', () => {
    it('defaults to undefined (callers fall back to cwd)', () => {
      expect(resolveLocal(undefined).workspaceRoot).toBeUndefined();
    });
    it('env overrides file', () => {
      process.env.WORKSPACE_ROOT = '/env/root';
      expect(resolveLocal({ workspaceRoot: '/file/root' }).workspaceRoot).toBe(
        '/env/root'
      );
    });
  });

  describe('ALLOWED_PATHS -> local.allowedPaths (default [])', () => {
    it('defaults to empty list', () => {
      expect(resolveLocal(undefined).allowedPaths).toEqual([]);
    });
    it('parses a comma-separated env list', () => {
      process.env.ALLOWED_PATHS = '/a, /b ,/c';
      expect(resolveLocal(undefined).allowedPaths).toEqual(['/a', '/b', '/c']);
    });
    it('uses .octocoderc JSON array when no env', () => {
      expect(resolveLocal({ allowedPaths: ['/x', '/y'] }).allowedPaths).toEqual(
        ['/x', '/y']
      );
    });
  });

  describe('TOOLS_TO_RUN / ENABLE_TOOLS / DISABLE_TOOLS -> tools.*', () => {
    it('default to null (no filtering)', () => {
      const t = resolveTools(undefined);
      expect(t.enabled).toBeNull();
      expect(t.enableAdditional).toBeNull();
      expect(t.disabled).toBeNull();
    });
    it('TOOLS_TO_RUN env populates the strict whitelist', () => {
      process.env.TOOLS_TO_RUN = 'ghSearchCode,localSearchCode';
      expect(resolveTools(undefined).enabled).toEqual([
        'ghSearchCode',
        'localSearchCode',
      ]);
    });
    it('ENABLE_TOOLS and DISABLE_TOOLS env populate add/remove lists', () => {
      process.env.ENABLE_TOOLS = 'ghCloneRepo';
      process.env.DISABLE_TOOLS = 'npmSearch';
      const t = resolveTools(undefined);
      expect(t.enableAdditional).toEqual(['ghCloneRepo']);
      expect(t.disabled).toEqual(['npmSearch']);
    });
  });

  describe('REQUEST_TIMEOUT -> network.timeout (default 30000, clamp 5000..300000)', () => {
    it('defaults to 30000', () => {
      expect(resolveNetwork(undefined).timeout).toBe(30000);
    });
    it('clamps below-min up to 5000', () => {
      process.env.REQUEST_TIMEOUT = '100';
      expect(resolveNetwork(undefined).timeout).toBe(5000);
    });
    it('clamps above-max down to 300000', () => {
      process.env.REQUEST_TIMEOUT = '999999';
      expect(resolveNetwork(undefined).timeout).toBe(300000);
    });
    it('accepts an in-range value', () => {
      process.env.REQUEST_TIMEOUT = '45000';
      expect(resolveNetwork(undefined).timeout).toBe(45000);
    });
  });

  describe('MAX_RETRIES -> network.maxRetries (default 3, clamp 0..10)', () => {
    it('defaults to 3', () => {
      expect(resolveNetwork(undefined).maxRetries).toBe(3);
    });
    it('clamps above-max down to 10', () => {
      process.env.MAX_RETRIES = '99';
      expect(resolveNetwork(undefined).maxRetries).toBe(10);
    });
    it('clamps negative up to 0', () => {
      process.env.MAX_RETRIES = '-5';
      expect(resolveNetwork(undefined).maxRetries).toBe(0);
    });
  });

  describe('OCTOCODE_LSP_CONFIG -> lsp.configPath (default unset)', () => {
    it('defaults to undefined (unset)', () => {
      expect(resolveLsp(undefined).configPath).toBeUndefined();
    });
    it('env overrides file', () => {
      process.env.OCTOCODE_LSP_CONFIG = '/env/lsp.json';
      expect(resolveLsp({ configPath: '/file/lsp.json' }).configPath).toBe(
        '/env/lsp.json'
      );
    });
  });

  describe('OCTOCODE_OUTPUT_FORMAT -> output.format (default yaml; invalid -> yaml)', () => {
    it('defaults to yaml', () => {
      expect(resolveOutput(undefined).format).toBe('yaml');
    });
    it('accepts json', () => {
      process.env.OCTOCODE_OUTPUT_FORMAT = 'json';
      expect(resolveOutput(undefined).format).toBe('json');
    });
    it('falls back to yaml for an invalid value', () => {
      process.env.OCTOCODE_OUTPUT_FORMAT = 'xml';
      expect(resolveOutput(undefined).format).toBe('yaml');
    });
  });

  describe('OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH -> output.pagination (default 20000, clamp 1000..50000)', () => {
    it('defaults to 20000', () => {
      expect(resolveOutput(undefined).pagination.defaultCharLength).toBe(20000);
    });
    it('clamps below-min up to 1000', () => {
      process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '10';
      expect(resolveOutput(undefined).pagination.defaultCharLength).toBe(1000);
    });
    it('clamps above-max down to 50000', () => {
      process.env.OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH = '999999';
      expect(resolveOutput(undefined).pagination.defaultCharLength).toBe(50000);
    });
  });

  describe('runtime surface: local.enabled defaults on; clone defaults differ by flow', () => {
    describe('CLI surface', () => {
      beforeEach(() => setRuntimeSurface('cli'));

      it('local defaults to ENABLED', () => {
        expect(resolveLocal(undefined).enabled).toBe(true);
      });

      it('ENABLE_LOCAL=false disables local tools', () => {
        process.env.ENABLE_LOCAL = 'false';
        expect(resolveLocal(undefined).enabled).toBe(false);
      });

      it('.octocoderc local.enabled=false disables local tools', () => {
        expect(resolveLocal({ enabled: false }).enabled).toBe(false);
      });

      it('clone defaults to ENABLED', () => {
        expect(resolveLocal(undefined).enableClone).toBe(true);
      });

      it('ENABLE_CLONE=false still disables clone', () => {
        process.env.ENABLE_CLONE = 'false';
        expect(resolveLocal(undefined).enableClone).toBe(false);
      });

      it('.octocoderc enableClone=false still disables clone', () => {
        expect(resolveLocal({ enableClone: false }).enableClone).toBe(false);
      });
    });

    describe('MCP surface (default)', () => {
      beforeEach(() => setRuntimeSurface('mcp'));

      it('local defaults on and honors ENABLE_LOCAL=false', () => {
        expect(resolveLocal(undefined).enabled).toBe(true);
        process.env.ENABLE_LOCAL = 'false';
        expect(resolveLocal(undefined).enabled).toBe(false);
      });

      it('ENABLE_LOCAL=true leaves local tools enabled', () => {
        process.env.ENABLE_LOCAL = 'true';
        expect(resolveLocal(undefined).enabled).toBe(true);
      });

      it('.octocoderc local.enabled=true enables local tools', () => {
        expect(resolveLocal({ enabled: true }).enabled).toBe(true);
      });

      it('ENABLE_LOCAL=false disables, even when file config enables local', () => {
        process.env.ENABLE_LOCAL = 'false';
        expect(resolveLocal({ enabled: true }).enabled).toBe(false);
      });

      it('clone defaults to DISABLED', () => {
        expect(resolveLocal(undefined).enableClone).toBe(false);
      });

      it('ENABLE_CLONE=true enables clone', () => {
        process.env.ENABLE_CLONE = 'true';
        expect(resolveLocal(undefined).enableClone).toBe(true);
      });
    });
  });
});
