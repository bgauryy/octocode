import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  checkCommandAvailability,
  checkAllCommandsAvailability,
  getMissingCommandError,
  clearAvailabilityCache,
  REQUIRED_COMMANDS,
} from '../../src/utils/exec/commandAvailability.js';

describe('commandAvailability', () => {
  beforeEach(() => {
    clearAvailabilityCache();
  });

  afterEach(() => {
    clearAvailabilityCache();
  });

  describe('checkCommandAvailability', () => {
    it('should check rg availability', async () => {
      const result = await checkCommandAvailability('rg');

      expect(result.command).toBe('rg');
      expect(typeof result.available).toBe('boolean');
    });

    it('should check find availability', async () => {
      const result = await checkCommandAvailability('find');

      expect(result.command).toBe('find');
      expect(typeof result.available).toBe('boolean');
    });

    it('should check ls availability', async () => {
      const result = await checkCommandAvailability('ls');

      expect(result.command).toBe('ls');
      expect(typeof result.available).toBe('boolean');
    });

    it('should cache results by default', async () => {
      const result1 = await checkCommandAvailability('ls');
      const result2 = await checkCommandAvailability('ls');

      expect(result1).toBe(result2);
    });

    it('should bypass cache with forceCheck', async () => {
      const result1 = await checkCommandAvailability('ls');
      const result2 = await checkCommandAvailability('ls', true);

      expect(result2.command).toBe(result1.command);
      expect(result2.available).toBe(result1.available);
    });

    it('should return error message when command is not available', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi
        .spyOn(spawnModule, 'spawnCheckSuccess')
        .mockResolvedValue(false);

      clearAvailabilityCache();

      const result = await checkCommandAvailability('rg', true);

      expect(result.available).toBe(false);
      expect(result.error).toContain('bundled binary is unavailable');

      spawnSpy.mockRestore();
    });

    it('should handle spawn errors gracefully', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi
        .spyOn(spawnModule, 'spawnCheckSuccess')
        .mockRejectedValue(new Error('Spawn failed'));

      clearAvailabilityCache();

      const result = await checkCommandAvailability('rg', true);

      expect(result.available).toBe(false);
      expect(result.error).toContain('Spawn failed');

      spawnSpy.mockRestore();
    });

    it('should handle non-Error spawn failures', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi
        .spyOn(spawnModule, 'spawnCheckSuccess')
        .mockRejectedValue('string error');

      clearAvailabilityCache();

      const result = await checkCommandAvailability('rg', true);

      expect(result.available).toBe(false);
      expect(result.error).toContain('Failed to check');

      spawnSpy.mockRestore();
    });
  });

  describe('checkAllCommandsAvailability', () => {
    it('should check all required commands', async () => {
      const results = await checkAllCommandsAvailability();

      expect(results.has('rg')).toBe(true);
      expect(results.has('find')).toBe(true);
      expect(results.has('ls')).toBe(true);

      for (const [command, result] of results) {
        expect(result.command).toBe(command);
        expect(typeof result.available).toBe('boolean');
      }
    });

    it('should return correct command in each result', async () => {
      const results = await checkAllCommandsAvailability();

      expect(results.get('rg')?.command).toBe('rg');
      expect(results.get('find')?.command).toBe('find');
      expect(results.get('ls')?.command).toBe('ls');
    });
  });

  describe('getMissingCommandError', () => {
    it('should return install instructions for rg', () => {
      const error = getMissingCommandError('rg');

      expect(error).toContain('ripgrep');
      expect(error).toContain('@vscode/ripgrep');
    });

    it('should return install instructions for find', () => {
      const error = getMissingCommandError('find');

      expect(error).toContain('find');
      expect(error).toMatch(/PATH|Git Bash|WSL|Unix/);
    });

    it('should return install instructions for ls', () => {
      const error = getMissingCommandError('ls');

      expect(error).toContain('ls');
      expect(error).toMatch(/PATH|Git Bash|WSL|Unix/);
    });
  });

  describe('clearAvailabilityCache', () => {
    it('should allow clearing the cache', () => {
      expect(() => clearAvailabilityCache()).not.toThrow();
    });

    it('should clear cached results', async () => {
      await checkCommandAvailability('ls');

      clearAvailabilityCache();

      const result = await checkCommandAvailability('ls', true);
      expect(result.command).toBe('ls');
    });
  });

  describe('POSIX command fast path', () => {
    it('should return available for find without calling spawnCheckSuccess on non-Windows', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi.spyOn(spawnModule, 'spawnCheckSuccess');

      clearAvailabilityCache();

      const result = await checkCommandAvailability('find', true);

      if (process.platform !== 'win32') {
        expect(result.available).toBe(true);
        expect(spawnSpy).not.toHaveBeenCalled();
      }

      spawnSpy.mockRestore();
    });

    it('should return available for ls without calling spawnCheckSuccess on non-Windows', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi.spyOn(spawnModule, 'spawnCheckSuccess');

      clearAvailabilityCache();

      const result = await checkCommandAvailability('ls', true);

      if (process.platform !== 'win32') {
        expect(result.available).toBe(true);
        expect(spawnSpy).not.toHaveBeenCalled();
      }

      spawnSpy.mockRestore();
    });

    it('should still call spawnCheckSuccess for rg (not POSIX)', async () => {
      const spawnModule = await import('../../src/utils/exec/spawn.js');
      const spawnSpy = vi
        .spyOn(spawnModule, 'spawnCheckSuccess')
        .mockResolvedValue(true);

      clearAvailabilityCache();

      await checkCommandAvailability('rg', true);

      expect(spawnSpy).toHaveBeenCalled();

      spawnSpy.mockRestore();
    });
  });

  describe('REQUIRED_COMMANDS', () => {
    it('should have required commands defined', () => {
      expect(REQUIRED_COMMANDS.rg).toBeDefined();
      expect(REQUIRED_COMMANDS.find).toBeDefined();
      expect(REQUIRED_COMMANDS.ls).toBeDefined();
    });

    it('should not include grep (fallback removed)', () => {
      expect(
        Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'grep')
      ).toBe(false);
    });

    it('should have correct tool names', () => {
      expect(REQUIRED_COMMANDS.rg.tool).toBe('localSearchCode');
      expect(REQUIRED_COMMANDS.find.tool).toBe('localFindFiles');
      expect(REQUIRED_COMMANDS.ls.tool).toBe('localViewStructure');
    });

    it('should have correct command names', () => {
      expect(REQUIRED_COMMANDS.rg.name).toBe('ripgrep');
      expect(REQUIRED_COMMANDS.find.name).toBe('find');
      expect(REQUIRED_COMMANDS.ls.name).toBe('ls');
    });

    it('should have version flags', () => {
      expect(REQUIRED_COMMANDS.rg.versionFlag).toBe('--version');
      expect(REQUIRED_COMMANDS.find.versionFlag).toBe('--version');
      expect(REQUIRED_COMMANDS.ls.versionFlag).toBe('--version');
    });
  });

  describe('OCTOCODE_COMMAND_CHECK_TIMEOUT_MS', () => {
    const originalEnv = process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS;

    afterEach(() => {
      if (originalEnv !== undefined) {
        process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS = originalEnv;
      } else {
        delete process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS;
      }
      vi.restoreAllMocks();
    });

    it('should default to 5000ms when env var is not set', async () => {
      delete process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS;
      vi.resetModules();
      const mod = await import('../../src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
      expect(mod.REQUIRED_COMMANDS).toBeDefined();
    });

    it('should accept custom timeout from env var', async () => {
      process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS = '10000';
      vi.resetModules();
      const mod = await import('../../src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
    });

    it('should fall back to 5000ms for invalid env var', async () => {
      process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS = 'invalid';
      vi.resetModules();
      const mod = await import('../../src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
    });
  });
});
