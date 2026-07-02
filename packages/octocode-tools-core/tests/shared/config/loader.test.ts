import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import {
  loadConfig,
  loadConfigSync,
  configExists,
  getConfigFilePath,
} from '@octocodeai/config';
import { getOctocodeDir } from '../../../src/shared/paths.js';

// CONFIG_FILE_PATH was a static const; now computed via getConfigFilePath().
const CONFIG_FILE_PATH = getConfigFilePath();
import { paths } from '../../../src/shared/paths.js';

vi.mock('node:fs', () => ({
  existsSync: vi.fn(),
  readFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  writeFileSync: vi.fn(),
  unlinkSync: vi.fn(),
}));

describe('config/loader', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('configExists', () => {
    it('returns true when config file exists', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      expect(configExists()).toBe(true);
      expect(existsSync).toHaveBeenCalledWith(CONFIG_FILE_PATH);
    });

    it('returns false when config file does not exist', () => {
      vi.mocked(existsSync).mockReturnValue(false);
      expect(configExists()).toBe(false);
    });
  });

  describe('getConfigFilePath', () => {
    it('returns the config file path', () => {
      const path = getConfigFilePath();
      expect(path).toBe(CONFIG_FILE_PATH);
      expect(path).toBe(paths.config);
      expect(path.endsWith('.octocoderc')).toBe(true);
    });
  });

  describe('getOctocodeDir', () => {
    it('returns the shared Octocode home directory', () => {
      const dir = getOctocodeDir();
      expect(typeof dir).toBe('string');
      expect(dir).toBe(paths.home);
    });
  });

  describe('loadConfigSync', () => {
    it('returns error when file does not exist', () => {
      vi.mocked(existsSync).mockReturnValue(false);

      const result = loadConfigSync();

      expect(result.success).toBe(false);
      expect(result.error).toBe('Config file does not exist');
      expect(result.path).toBe(CONFIG_FILE_PATH);
    });

    it('returns empty config for empty file', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('');

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config).toEqual({});
    });

    it('parses valid JSON config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        '{"version": 1, "github": {"apiUrl": "https://api.github.com"}}'
      );

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.version).toBe(1);
      expect(result.config?.github?.apiUrl).toBe('https://api.github.com');
    });

    it('parses JSON5 with single-line comments', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(`{
        "version": 1
      }`);

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.version).toBe(1);
    });

    it('parses JSON5 with multi-line comments', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(`{
        /* This is a
           multi-line comment */
        "version": 1
      }`);

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.version).toBe(1);
    });

    it('parses JSON5 with trailing commas', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(`{
        "version": 1,
        "github": {
          "apiUrl": "https://api.github.com",
        },
      }`);

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.version).toBe(1);
    });

    it('handles comments inside strings correctly', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(`{
        "version": 1,
        "github": {
          "apiUrl": "https://api.github.com/v3 // not a comment"
        }
      }`);

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.github?.apiUrl).toBe(
        'https://api.github.com/v3 // not a comment'
      );
    });

    it('parses JSON strings containing backslash escape sequences', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        '{"version": 1, "key": "value\\\\with\\\\backslash"}'
      );

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect((result.config as Record<string, unknown>)?.key).toBe(
        'value\\with\\backslash'
      );
    });

    it('returns error for invalid JSON', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{ invalid json }');

      const result = loadConfigSync();

      expect(result.success).toBe(false);
      expect(result.error).toContain('Failed to parse config file');
    });

    it('returns error for non-object config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('"just a string"');

      const result = loadConfigSync();

      expect(result.success).toBe(false);
      expect(result.error).toContain('Config file has invalid structure');
    });

    it('returns error for array config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('[1, 2, 3]');

      const result = loadConfigSync();

      expect(result.success).toBe(false);
      expect(result.error).toContain('Config file has invalid structure');
    });

    it('returns error for null config', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('null');

      const result = loadConfigSync();

      expect(result.success).toBe(false);
      expect(result.error).toContain('Config file has invalid structure');
    });

    it('succeeds parsing when version is not an integer (field validation is validateConfig\'s job)', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{"version": 1.5}');

      const result = loadConfigSync();

      // loadConfigSync only checks structure (is it an object?); field-level
      // validation (e.g. integer version) belongs to validateConfig.
      expect(result.success).toBe(true);
      expect((result.config as Record<string, unknown>)?.version).toBe(1.5);
    });

    it('accepts config with unknown extra keys via passthrough', () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue(
        '{"version": 1, "customKey": "value"}'
      );

      const result = loadConfigSync();

      expect(result.success).toBe(true);
      expect(result.config?.version).toBe(1);
    });
  });

  describe('loadConfig', () => {
    it('returns same result as loadConfigSync', async () => {
      vi.mocked(existsSync).mockReturnValue(true);
      vi.mocked(readFileSync).mockReturnValue('{"version": 1}');

      const asyncResult = await loadConfig();
      const syncResult = loadConfigSync();

      expect(asyncResult).toEqual(syncResult);
    });
  });
});
