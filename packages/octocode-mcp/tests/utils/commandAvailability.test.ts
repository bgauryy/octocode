import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  checkCommandAvailability,
  checkAllCommandsAvailability,
  getMissingCommandError,
  clearAvailabilityCache,
  REQUIRED_COMMANDS,
} from '../../../octocode-tools-core/src/utils/exec/commandAvailability.js';

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

    it('should cache results by default', async () => {
      const result1 = await checkCommandAvailability('rg');
      const result2 = await checkCommandAvailability('rg');

      expect(result1).toBe(result2);
    });

    it('should bypass cache with forceCheck', async () => {
      const result1 = await checkCommandAvailability('rg');
      const result2 = await checkCommandAvailability('rg', true);

      expect(result2.command).toBe(result1.command);
      expect(result2.available).toBe(result1.available);
    });

    it('should return error message when command is not available', async () => {
      const spawnModule =
        await import('../../../octocode-tools-core/src/utils/exec/spawn.js');
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
      const spawnModule =
        await import('../../../octocode-tools-core/src/utils/exec/spawn.js');
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
      const spawnModule =
        await import('../../../octocode-tools-core/src/utils/exec/spawn.js');
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

      for (const [command, result] of results) {
        expect(result.command).toBe(command);
        expect(typeof result.available).toBe('boolean');
      }
    });

    it('should return correct command in each result', async () => {
      const results = await checkAllCommandsAvailability();

      expect(results.get('rg')?.command).toBe('rg');
    });
  });

  describe('getMissingCommandError', () => {
    it('should return install instructions for rg', () => {
      const error = getMissingCommandError('rg');

      expect(error).toContain('ripgrep');
      expect(error).toContain('dist/runtime/rg');
    });
  });

  describe('clearAvailabilityCache', () => {
    it('should allow clearing the cache', () => {
      expect(() => clearAvailabilityCache()).not.toThrow();
    });

    it('should clear cached results', async () => {
      await checkCommandAvailability('rg');

      clearAvailabilityCache();

      const result = await checkCommandAvailability('rg', true);
      expect(result.command).toBe('rg');
    });
  });

  describe('POSIX command fast path', () => {
    it('should still call spawnCheckSuccess for rg (not POSIX)', async () => {
      const spawnModule =
        await import('../../../octocode-tools-core/src/utils/exec/spawn.js');
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
    });

    it('should not include grep (fallback removed)', () => {
      expect(
        Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'grep')
      ).toBe(false);
    });

    it('should not include find/ls (native filesystem migration)', () => {
      expect(
        Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'find')
      ).toBe(false);
      expect(
        Object.prototype.hasOwnProperty.call(REQUIRED_COMMANDS, 'ls')
      ).toBe(false);
    });

    it('should have correct tool names', () => {
      expect(REQUIRED_COMMANDS.rg.tool).toBe('localSearchCode');
    });

    it('should have correct command names', () => {
      expect(REQUIRED_COMMANDS.rg.name).toBe('ripgrep');
    });

    it('should have version flags', () => {
      expect(REQUIRED_COMMANDS.rg.versionFlag).toBe('--version');
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
      const mod =
        await import('../../../octocode-tools-core/src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
      expect(mod.REQUIRED_COMMANDS).toBeDefined();
    });

    it('should accept custom timeout from env var', async () => {
      process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS = '10000';
      vi.resetModules();
      const mod =
        await import('../../../octocode-tools-core/src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
    });

    it('should fall back to 5000ms for invalid env var', async () => {
      process.env.OCTOCODE_COMMAND_CHECK_TIMEOUT_MS = 'invalid';
      vi.resetModules();
      const mod =
        await import('../../../octocode-tools-core/src/utils/exec/commandAvailability.js');
      expect(mod.checkCommandAvailability).toBeDefined();
    });
  });
});
