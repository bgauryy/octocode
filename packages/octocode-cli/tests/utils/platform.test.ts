import { describe, it, expect, vi, beforeEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

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
  },
  existsSync: vi.fn(),
}));

vi.mock('node:child_process', () => ({
  spawnSync: vi.fn(),
}));

describe('Platform Utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Platform Detection (re-exports from octocode-shared)', () => {
    it('should re-export platform detection from octocode-shared', async () => {
      const { isMac, isWindows, HOME } =
        await import('../../src/utils/platform.js');
      const { isLinux } = await import('octocode-shared');
      expect(isMac).toBe(true);
      expect(isWindows).toBe(false);
      expect(isLinux).toBe(false);
      expect(HOME).toBe('/Users/test');
    });

    it('should expose getPlatformName from octocode-shared', async () => {
      const { getPlatformName } = await import('octocode-shared');
      expect(getPlatformName()).toBe('macOS');
    });

    it('should expose getArchitecture from octocode-shared', async () => {
      const { getArchitecture } = await import('octocode-shared');
      expect(getArchitecture()).toBe('arm64');
    });

    it('should re-export getAppDataPath from octocode-shared', async () => {
      const { getAppDataPath } = await import('../../src/utils/platform.js');
      expect(getAppDataPath()).toBe('/Users/test');
    });

    it('should expose getLocalAppDataPath from octocode-shared', async () => {
      const { getLocalAppDataPath } = await import('octocode-shared');
      expect(getLocalAppDataPath()).toBe('/Users/test');
    });
  });

  describe('isGitRelated', () => {
    it('should detect .git directory', async () => {
      const { isGitRelated } = await import('../../src/utils/platform.js');
      expect(isGitRelated('.git')).toBe(true);
      expect(isGitRelated('/path/to/.git')).toBe(true);
    });

    it('should detect other VCS directories', async () => {
      const { isGitRelated } = await import('../../src/utils/platform.js');
      expect(isGitRelated('.svn')).toBe(true);
      expect(isGitRelated('.hg')).toBe(true);
      expect(isGitRelated('.bzr')).toBe(true);
    });

    it('should return false for non-VCS directories', async () => {
      const { isGitRelated } = await import('../../src/utils/platform.js');
      expect(isGitRelated('src')).toBe(false);
      expect(isGitRelated('node_modules')).toBe(false);
    });
  });

  describe('isIDERelated', () => {
    it('should detect .vscode directory', async () => {
      const { isIDERelated } = await import('../../src/utils/platform.js');
      expect(isIDERelated('.vscode')).toBe(true);
      expect(isIDERelated('/path/to/.vscode')).toBe(true);
    });

    it('should detect JetBrains .idea directory', async () => {
      const { isIDERelated } = await import('../../src/utils/platform.js');
      expect(isIDERelated('.idea')).toBe(true);
    });

    it('should detect Visual Studio .vs directory', async () => {
      const { isIDERelated } = await import('../../src/utils/platform.js');
      expect(isIDERelated('.vs')).toBe(true);
    });

    it('should return false for non-IDE directories', async () => {
      const { isIDERelated } = await import('../../src/utils/platform.js');
      expect(isIDERelated('src')).toBe(false);
      expect(isIDERelated('.git')).toBe(false);
    });
  });

  describe('isIDEOrGitPath', () => {
    it('should return true for both IDE and Git paths', async () => {
      const { isIDEOrGitPath } = await import('../../src/utils/platform.js');
      expect(isIDEOrGitPath('.git')).toBe(true);
      expect(isIDEOrGitPath('.vscode')).toBe(true);
      expect(isIDEOrGitPath('.idea')).toBe(true);
    });

    it('should return false for regular paths', async () => {
      const { isIDEOrGitPath } = await import('../../src/utils/platform.js');
      expect(isIDEOrGitPath('src')).toBe(false);
      expect(isIDEOrGitPath('node_modules')).toBe(false);
    });
  });

  describe('isInsideGitRepo', () => {
    it('should return true when .git exists in current directory', async () => {
      vi.mocked(fs.existsSync).mockImplementation(p => {
        return p === '/Users/test/project/.git';
      });

      const { isInsideGitRepo } = await import('../../src/utils/platform.js');
      expect(isInsideGitRepo('/Users/test/project')).toBe(true);
    });

    it('should return true when .git exists in parent directory', async () => {
      vi.mocked(fs.existsSync).mockImplementation(p => {
        return p === '/Users/test/project/.git';
      });

      const { isInsideGitRepo } = await import('../../src/utils/platform.js');
      expect(isInsideGitRepo('/Users/test/project/src/utils')).toBe(true);
    });

    it('should return false when no .git exists', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { isInsideGitRepo } = await import('../../src/utils/platform.js');
      expect(isInsideGitRepo('/Users/test/not-a-repo')).toBe(false);
    });
  });

  describe('findGitRoot', () => {
    it('should return the git root directory', async () => {
      vi.mocked(fs.existsSync).mockImplementation(p => {
        return p === '/Users/test/project/.git';
      });

      const { findGitRoot } = await import('../../src/utils/platform.js');
      expect(findGitRoot('/Users/test/project/src/utils')).toBe(
        '/Users/test/project'
      );
    });

    it('should return null when no git root found', async () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const { findGitRoot } = await import('../../src/utils/platform.js');
      expect(findGitRoot('/Users/test/not-a-repo')).toBe(null);
    });

    it('should return repo root at filesystem root when .git exists only there', async () => {
      const root = path.parse(path.resolve('/')).root;
      const rootGit = path.join(root, '.git');
      vi.mocked(fs.existsSync).mockImplementation(
        (p: fs.PathLike) => String(p) === rootGit
      );

      const { findGitRoot } = await import('../../src/utils/platform.js');
      expect(findGitRoot(path.join('/tmp', 'nested', 'deep', 'project'))).toBe(
        path.resolve(root)
      );
    });

    it('should return null when existsSync throws', async () => {
      vi.mocked(fs.existsSync).mockImplementation(() => {
        throw new Error('EACCES');
      });

      const { findGitRoot } = await import('../../src/utils/platform.js');
      expect(findGitRoot('/Users/test/any')).toBe(null);
    });
  });

  describe('clearScreen', () => {
    it('should write clear sequence to stdout', async () => {
      const writeSpy = vi.spyOn(process.stdout, 'write').mockReturnValue(true);

      const { clearScreen } = await import('../../src/utils/platform.js');
      clearScreen();

      expect(writeSpy).toHaveBeenCalledWith('\x1b[2J\x1b[3J\x1b[H');
      writeSpy.mockRestore();
    });
  });

  describe('openFile', () => {
    it('should open file with specified editor', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { openFile } = await import('../../src/utils/platform.js');
      const result = openFile('/path/to/file.txt', 'vim');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'vim',
        ['/path/to/file.txt'],
        expect.any(Object)
      );
    });

    it('should use open command on macOS', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { openFile } = await import('../../src/utils/platform.js');
      const result = openFile('/path/to/file.txt');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'open',
        ['/path/to/file.txt'],
        expect.any(Object)
      );
    });

    it('should return false on error', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockImplementation(() => {
        throw new Error('Command failed');
      });

      const { openFile } = await import('../../src/utils/platform.js');
      const result = openFile('/path/to/file.txt');

      expect(result).toBe(false);
    });
  });

  describe('openInEditor', () => {
    it('should open file in cursor', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { openInEditor } = await import('../../src/utils/platform.js');
      const result = openInEditor('/path/to/file.txt', 'cursor');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'cursor',
        ['/path/to/file.txt'],
        expect.any(Object)
      );
    });

    it('should open file in vscode', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { openInEditor } = await import('../../src/utils/platform.js');
      const result = openInEditor('/path/to/file.txt', 'vscode');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'code',
        ['/path/to/file.txt'],
        expect.any(Object)
      );
    });

    it('should use default opener for default option', async () => {
      const { spawnSync } = await import('node:child_process');
      vi.mocked(spawnSync).mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });

      const { openInEditor } = await import('../../src/utils/platform.js');
      const result = openInEditor('/path/to/file.txt', 'default');

      expect(result).toBe(true);

      expect(spawnSync).toHaveBeenCalledWith(
        'open',
        ['/path/to/file.txt'],
        expect.any(Object)
      );
    });
  });

  describe('openFile cross-platform defaults', () => {
    it('should use cmd /c start on Windows', async () => {
      vi.resetModules();
      vi.doMock('octocode-shared', () => ({
        isWindows: true,
        isMac: false,
        isLinux: false,
        HOME: 'C:\\Users\\test',
        getAppDataPath: vi.fn(() => 'C:\\Users\\test\\AppData\\Roaming'),
        getLocalAppDataPath: vi.fn(() => 'C:\\Users\\test\\AppData\\Local'),
        getPlatformName: vi.fn(() => 'Windows'),
        getArchitecture: vi.fn(() => 'x64'),
      }));
      vi.doMock('node:fs', () => ({
        default: { existsSync: vi.fn() },
        existsSync: vi.fn(),
      }));
      const spawnSync = vi.fn().mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });
      vi.doMock('node:child_process', () => ({ spawnSync }));

      const { openFile } = await import('../../src/utils/platform.js');
      const result = openFile('C:\\path\\to\\file.txt');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'cmd',
        ['/c', 'start', '""', 'C:\\path\\to\\file.txt'],
        expect.objectContaining({ shell: true, stdio: 'ignore' })
      );
    });

    it('should use xdg-open on Linux', async () => {
      vi.resetModules();
      vi.doMock('octocode-shared', () => ({
        isWindows: false,
        isMac: false,
        isLinux: true,
        HOME: '/home/test',
        getAppDataPath: vi.fn(() => '/home/test'),
        getLocalAppDataPath: vi.fn(() => '/home/test'),
        getPlatformName: vi.fn(() => 'Linux'),
        getArchitecture: vi.fn(() => 'x64'),
      }));
      vi.doMock('node:fs', () => ({
        default: { existsSync: vi.fn() },
        existsSync: vi.fn(),
      }));
      const spawnSync = vi.fn().mockReturnValue({
        status: 0,
        stdout: '',
        stderr: '',
        pid: 123,
        output: [],
        signal: null,
      });
      vi.doMock('node:child_process', () => ({ spawnSync }));

      const { openFile } = await import('../../src/utils/platform.js');
      const result = openFile('/path/to/file.txt');

      expect(result).toBe(true);
      expect(spawnSync).toHaveBeenCalledWith(
        'xdg-open',
        ['/path/to/file.txt'],
        expect.objectContaining({ stdio: 'ignore' })
      );
    });
  });
});
