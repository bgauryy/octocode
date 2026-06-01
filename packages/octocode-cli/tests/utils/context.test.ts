import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('../../src/utils/shell.js', () => ({
  runCommand: vi.fn(),
}));

vi.mock('node:os', async importOriginal => {
  const actual = await importOriginal<typeof import('node:os')>();
  return {
    ...actual,
    default: {
      ...actual,
      homedir: vi.fn(),
    },
    homedir: vi.fn(),
  };
});

describe('Context Utilities', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
  });

  describe('getAppContext', () => {
    it('should return context with cwd, ide, and git info', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand)
        .mockReturnValueOnce({
          success: true,
          stdout: '/Users/test/project',
          stderr: '',
          exitCode: 0,
        })
        .mockReturnValueOnce({
          success: true,
          stdout: 'main',
          stderr: '',
          exitCode: 0,
        });

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.cwd).toBe('~/project');
      expect(context.ide).toBeDefined();
      expect(context.git).toBeDefined();
      expect(context.git?.branch).toBe('main');

      process.cwd = originalCwd;
    });

    it('should return shortened cwd with ~', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: 'Not a git repo',
        exitCode: 1,
      });

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/projects/myapp');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.cwd).toBe('~/projects/myapp');

      process.cwd = originalCwd;
    });

    it('should return full path when not in home directory', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: 'Not a git repo',
        exitCode: 1,
      });

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/opt/app');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.cwd).toBe('/opt/app');

      process.cwd = originalCwd;
    });

    it('should detect Cursor IDE', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 1,
      });

      const originalEnv = { ...process.env };
      process.env.CURSOR_AGENT = 'true';

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.ide).toBe('Cursor');

      process.env = originalEnv;
      process.cwd = originalCwd;
    });

    it('should detect VS Code IDE', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 1,
      });

      const originalEnv = { ...process.env };
      delete process.env.CURSOR_AGENT;
      delete process.env.CURSOR_TRACE_ID;
      process.env.TERM_PROGRAM = 'vscode';

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.ide).toBe('VS Code');

      process.env = originalEnv;
      process.cwd = originalCwd;
    });

    it('should detect Terminal when TERM_PROGRAM is Apple_Terminal', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 1,
      });

      const originalEnv = { ...process.env };
      delete process.env.CURSOR_AGENT;
      delete process.env.CURSOR_TRACE_ID;
      delete process.env.VSCODE_PID;
      process.env.TERM_PROGRAM = 'Apple_Terminal';

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.ide).toBe('Terminal');

      process.env = originalEnv;
      process.cwd = originalCwd;
    });

    it('should detect Terminal as fallback when no IDE env matches', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: '',
        exitCode: 1,
      });

      const originalEnv = { ...process.env };
      delete process.env.CURSOR_AGENT;
      delete process.env.CURSOR_TRACE_ID;
      delete process.env.VSCODE_PID;
      delete process.env.TERM_PROGRAM;

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.ide).toBe('Terminal');

      process.env = originalEnv;
      process.cwd = originalCwd;
    });

    it('should return undefined git when not in a repo', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand).mockReturnValue({
        success: false,
        stdout: '',
        stderr: 'Not a git repository',
        exitCode: 128,
      });

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.git).toBeUndefined();

      process.cwd = originalCwd;
    });

    it('should use HEAD when branch detection fails', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand)
        .mockReturnValueOnce({
          success: true,
          stdout: '/Users/test/project',
          stderr: '',
          exitCode: 0,
        })
        .mockReturnValueOnce({
          success: false,
          stdout: '',
          stderr: 'detached HEAD',
          exitCode: 1,
        });

      const originalCwd = process.cwd;
      process.cwd = vi.fn().mockReturnValue('/Users/test/project');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.git?.branch).toBe('HEAD');

      process.cwd = originalCwd;
    });

    it('should extract repo name from git root path', async () => {
      const os = await import('node:os');
      vi.mocked(os.homedir).mockReturnValue('/Users/test');

      const { runCommand } = await import('../../src/utils/shell.js');
      vi.mocked(runCommand)
        .mockReturnValueOnce({
          success: true,
          stdout: '/Users/test/projects/my-awesome-repo',
          stderr: '',
          exitCode: 0,
        })
        .mockReturnValueOnce({
          success: true,
          stdout: 'develop',
          stderr: '',
          exitCode: 0,
        });

      const originalCwd = process.cwd;
      process.cwd = vi
        .fn()
        .mockReturnValue('/Users/test/projects/my-awesome-repo');

      const { getAppContext } = await import('../../src/utils/context.js');
      const context = getAppContext();

      expect(context.git?.root).toBe('my-awesome-repo');
      expect(context.git?.branch).toBe('develop');

      process.cwd = originalCwd;
    });
  });
});
