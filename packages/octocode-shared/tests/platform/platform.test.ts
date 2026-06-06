import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import os from 'node:os';

import {
  isWindows,
  isMac,
  isLinux,
  HOME,
  getAppDataPath,
  getLocalAppDataPath,
  getPlatformName,
  getArchitecture,
} from '../../src/platform/platform.js';

describe('Platform Utilities', () => {
  describe('platform detection', () => {
    it('should export platform detection constants', () => {
      expect(typeof isWindows).toBe('boolean');
      expect(typeof isMac).toBe('boolean');
      expect(typeof isLinux).toBe('boolean');
    });

    it('should have at most one platform as true', () => {
      const truePlatforms = [isWindows, isMac, isLinux].filter(Boolean);
      expect(truePlatforms.length).toBeLessThanOrEqual(1);
    });

    it('should export HOME as user home directory', () => {
      expect(HOME).toBe(os.homedir());
    });
  });

  describe('getAppDataPath', () => {
    it('should return a valid path', () => {
      const appDataPath = getAppDataPath();
      expect(typeof appDataPath).toBe('string');
      expect(appDataPath.length).toBeGreaterThan(0);
    });

    it('should return HOME on non-Windows platforms', () => {
      if (!isWindows) {
        expect(getAppDataPath()).toBe(HOME);
      }
    });
  });

  describe('getLocalAppDataPath', () => {
    it('should return a valid path', () => {
      const localAppDataPath = getLocalAppDataPath();
      expect(typeof localAppDataPath).toBe('string');
      expect(localAppDataPath.length).toBeGreaterThan(0);
    });

    it('should return HOME on non-Windows platforms', () => {
      if (!isWindows) {
        expect(getLocalAppDataPath()).toBe(HOME);
      }
    });
  });

  describe('getPlatformName', () => {
    it('should return a readable platform name', () => {
      const platformName = getPlatformName();
      expect(typeof platformName).toBe('string');

      if (isMac) {
        expect(platformName).toBe('macOS');
      } else if (isWindows) {
        expect(platformName).toBe('Windows');
      } else if (isLinux) {
        expect(platformName).toBe('Linux');
      }
    });
  });

  describe('getArchitecture', () => {
    it('should return the system architecture', () => {
      const arch = getArchitecture();
      expect(typeof arch).toBe('string');
      expect(arch).toBe(os.arch());
    });
  });
});

describe('Platform Utilities (Windows-specific)', () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
    process.env = { ...originalEnv };
  });

  describe('getAppDataPath on Windows', () => {
    it('should return APPDATA env var when set', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'win32',
          homedir: () => 'C:\\Users\\TestUser',
        },
        platform: () => 'win32',
        homedir: () => 'C:\\Users\\TestUser',
      }));

      process.env.APPDATA = 'C:\\Users\\TestUser\\AppData\\Roaming';

      const { getAppDataPath } = await import('../../src/platform/platform.js');
      expect(getAppDataPath()).toBe('C:\\Users\\TestUser\\AppData\\Roaming');
    });

    it('should return fallback path when APPDATA is not set', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'win32',
          homedir: () => 'C:\\Users\\TestUser',
        },
        platform: () => 'win32',
        homedir: () => 'C:\\Users\\TestUser',
      }));

      delete process.env.APPDATA;

      const { getAppDataPath } = await import('../../src/platform/platform.js');
      expect(getAppDataPath()).toContain('AppData');
      expect(getAppDataPath()).toContain('Roaming');
    });
  });

  describe('getLocalAppDataPath on Windows', () => {
    it('should return LOCALAPPDATA env var when set', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'win32',
          homedir: () => 'C:\\Users\\TestUser',
        },
        platform: () => 'win32',
        homedir: () => 'C:\\Users\\TestUser',
      }));

      process.env.LOCALAPPDATA = 'C:\\Users\\TestUser\\AppData\\Local';

      const { getLocalAppDataPath } =
        await import('../../src/platform/platform.js');
      expect(getLocalAppDataPath()).toBe('C:\\Users\\TestUser\\AppData\\Local');
    });

    it('should return fallback path when LOCALAPPDATA is not set', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'win32',
          homedir: () => 'C:\\Users\\TestUser',
        },
        platform: () => 'win32',
        homedir: () => 'C:\\Users\\TestUser',
      }));

      delete process.env.LOCALAPPDATA;

      const { getLocalAppDataPath } =
        await import('../../src/platform/platform.js');
      expect(getLocalAppDataPath()).toContain('AppData');
      expect(getLocalAppDataPath()).toContain('Local');
    });
  });

  describe('getPlatformName for different platforms', () => {
    it('should return "Windows" for win32', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'win32',
          homedir: () => 'C:\\Users\\TestUser',
          arch: () => 'x64',
        },
        platform: () => 'win32',
        homedir: () => 'C:\\Users\\TestUser',
        arch: () => 'x64',
      }));

      const { getPlatformName } =
        await import('../../src/platform/platform.js');
      expect(getPlatformName()).toBe('Windows');
    });

    it('should return "Linux" for linux', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'linux',
          homedir: () => '/home/testuser',
          arch: () => 'x64',
        },
        platform: () => 'linux',
        homedir: () => '/home/testuser',
        arch: () => 'x64',
      }));

      const { getPlatformName } =
        await import('../../src/platform/platform.js');
      expect(getPlatformName()).toBe('Linux');
    });

    it('should return raw platform for unknown platforms', async () => {
      vi.doMock('node:os', () => ({
        default: {
          platform: () => 'freebsd',
          homedir: () => '/home/testuser',
          arch: () => 'x64',
        },
        platform: () => 'freebsd',
        homedir: () => '/home/testuser',
        arch: () => 'x64',
      }));

      const { getPlatformName } =
        await import('../../src/platform/platform.js');
      expect(getPlatformName()).toBe('freebsd');
    });
  });
});
