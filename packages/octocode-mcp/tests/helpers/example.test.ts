/**
 * Example Test File - Demonstrates Unified Test Helpers
 *
 * This file shows how to use the new unified test helpers for local tool testing.
 * Use this as a template for new test files.
 *
 * KEY INSIGHT: vi.hoisted() runs BEFORE any imports, so we must define all mocks
 * inline without using imported functions.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

const { fsMocks, execMocks, pathValidatorMocks, mocks, createStats, helpers } =
  vi.hoisted(() => {
    // Create all mock functions
    const lstatSync = vi.fn();
    const realpathSync = vi.fn();
    const readdir = vi.fn();
    const lstat = vi.fn();
    const stat = vi.fn();
    const readFile = vi.fn();
    const access = vi.fn();
    const safeExec = vi.fn();
    const checkCommandAvailability = vi.fn().mockResolvedValue({
      available: true,
      command: 'ls',
    });
    const getMissingCommandError = vi
      .fn()
      .mockReturnValue('Command not available');
    const validate = vi.fn();

    // Stats factory
    const createStats = (
      opts: {
        isFile?: boolean;
        isDir?: boolean;
        isSymlink?: boolean;
        size?: number;
        mtime?: Date;
      } = {}
    ) => ({
      isFile: () => opts.isFile ?? (!opts.isDir && !opts.isSymlink),
      isDirectory: () => opts.isDir ?? false,
      isSymbolicLink: () => opts.isSymlink ?? false,
      isBlockDevice: () => false,
      isCharacterDevice: () => false,
      isFIFO: () => false,
      isSocket: () => false,
      size: opts.size ?? 0,
      mtime: opts.mtime ?? new Date(),
      mode: 0o644,
    });

    // Dirent factory
    const createDirent = (
      name: string,
      type: 'file' | 'directory' | 'symlink' = 'file'
    ) => ({
      name,
      isFile: () => type === 'file',
      isDirectory: () => type === 'directory',
      isSymbolicLink: () => type === 'symlink',
      isBlockDevice: () => false,
      isCharacterDevice: () => false,
      isFIFO: () => false,
      isSocket: () => false,
    });

    return {
      // Mock module structures
      fsMocks: {
        default: {
          lstatSync,
          realpathSync,
          promises: { readdir, lstat, stat, readFile, access },
        },
        lstatSync,
        realpathSync,
        promises: { readdir, lstat, stat, readFile, access },
      },
      execMocks: { safeExec, checkCommandAvailability, getMissingCommandError },
      pathValidatorMocks: { pathValidator: { validate } },

      // Individual mocks for direct access
      mocks: {
        lstatSync,
        realpathSync,
        readdir,
        lstat,
        stat,
        readFile,
        access,
        safeExec,
        validate,
      },

      // Factories
      createStats,
      createDirent,

      // Helper functions
      helpers: {
        reset: () => {
          [
            lstatSync,
            realpathSync,
            readdir,
            lstat,
            stat,
            readFile,
            access,
            safeExec,
            validate,
          ].forEach(fn => fn.mockClear());
        },

        setupValidPath: (path = '/test/path') => {
          validate.mockReturnValue({ isValid: true, sanitizedPath: path });
        },

        setupInvalidPath: (error = 'Path not allowed') => {
          validate.mockReturnValue({ isValid: false, error });
        },

        setupExecSuccess: (stdout = '', stderr = '') => {
          safeExec.mockResolvedValue({
            success: true,
            code: 0,
            stdout,
            stderr,
          });
        },

        setupExecFailure: (stderr = 'Command failed', code = 1) => {
          safeExec.mockResolvedValue({
            success: false,
            code,
            stdout: '',
            stderr,
          });
        },

        setupLstatFile: (size = 1024, mtime = new Date()) => {
          const stats = createStats({ isFile: true, size, mtime });
          lstat.mockResolvedValue(stats);
          lstatSync.mockReturnValue(stats);
          stat.mockResolvedValue(stats);
        },

        setupLstatDir: (mtime = new Date()) => {
          const stats = createStats({ isDir: true, mtime });
          lstat.mockResolvedValue(stats);
          lstatSync.mockReturnValue(stats);
          stat.mockResolvedValue(stats);
        },

        setupReaddir: (
          files: Array<
            string | { name: string; type: 'file' | 'directory' | 'symlink' }
          >
        ) => {
          const names = files.map(f => (typeof f === 'string' ? f : f.name));
          readdir.mockImplementation(
            async (_path: string, opts?: { withFileTypes?: boolean }) => {
              if (opts?.withFileTypes) {
                return files.map(f =>
                  typeof f === 'string'
                    ? createDirent(f, 'file')
                    : createDirent(f.name, f.type)
                );
              }
              return names;
            }
          );
        },

        setupReadFile: (content: string) => {
          readFile.mockResolvedValue(content);
        },

        setupRealpath: (resolvedPath: string) => {
          realpathSync.mockReturnValue(resolvedPath);
        },

        // Output creators
        createLsOutput: (files: string[]) => files.join('\n'),

        createFindOutput: (paths: string[]) =>
          paths.length > 0 ? paths.join('\0') + '\0' : '',
      },
    };
  });

vi.mock('fs', () => fsMocks);
vi.mock('../../src/utils/exec/safe.js', () => ({
  safeExec: execMocks.safeExec,
}));

vi.mock('../../src/utils/exec/commandAvailability.js', () => ({
  checkCommandAvailability: execMocks.checkCommandAvailability,
  getMissingCommandError: execMocks.getMissingCommandError,
}));
vi.mock('octocode-security-utils/pathValidator', () => pathValidatorMocks);

const { viewStructure } =
  await import('../../src/tools/local_view_structure/local_view_structure.js');

describe('Example: Using Unified Test Helpers', () => {
  beforeEach(() => {
    // Reset all mocks to clean state before each test
    helpers.reset();

    // Set up default mocks for most tests
    helpers.setupValidPath('/workspace');
    helpers.setupRealpath('/workspace');
  });

  describe('Basic Directory Listing', () => {
    it('should list files from ls output', async () => {
      // Setup: ls returns file list
      helpers.setupExecSuccess(
        helpers.createLsOutput(['file1.ts', 'file2.ts', 'dir1'])
      );

      // Setup: lstat returns appropriate stats for each entry
      mocks.lstat.mockImplementation(async (path: string) => {
        if (path.includes('dir1')) {
          return createStats({ isDir: true });
        }
        return createStats({ isFile: true, size: 1024 });
      });

      const result = await viewStructure({
        path: '/workspace',
        researchGoal: 'Test directory listing',
      });

      expect(result.status).toBeUndefined();
      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });

    it('should handle empty directories', async () => {
      helpers.setupExecSuccess('');

      const result = await viewStructure({
        path: '/workspace',
      });

      expect(result.status).toBe('empty');
    });

    it('should handle command failures', async () => {
      helpers.setupExecFailure('Permission denied', 1);

      const result = await viewStructure({
        path: '/workspace',
      });

      expect(result.status).toBe('error');
    });
  });

  describe('Recursive Listing with Depth', () => {
    it('should use recursive walk for depth > 0', async () => {
      // Setup: readdir returns directory contents
      helpers.setupReaddir([
        { name: 'src', type: 'directory' },
        { name: 'package.json', type: 'file' },
      ]);

      // Setup: lstat returns appropriate stats
      mocks.lstat.mockImplementation(async (path: string) => {
        if (path.includes('src') || path.includes('workspace')) {
          return createStats({ isDir: true });
        }
        return createStats({ isFile: true, size: 100 });
      });

      const result = await viewStructure({
        path: '/workspace',
        depth: 1,
      });

      expect(result.status).toBeUndefined();
    });
  });

  describe('Path Validation', () => {
    it('should reject invalid paths', async () => {
      helpers.setupInvalidPath('Path outside workspace');

      const result = await viewStructure({
        path: '/etc/passwd',
      });

      expect(result.status).toBe('error');
    });

    it('should accept valid workspace paths', async () => {
      helpers.setupValidPath('/workspace/src');
      helpers.setupExecSuccess('');

      const result = await viewStructure({
        path: '/workspace/src',
      });

      // Empty but valid
      expect(result.status).toBe('empty');
    });
  });

  describe('Pagination', () => {
    it('should include pagination info', async () => {
      // Create many files for pagination
      const files = Array.from({ length: 25 }, (_, i) => `file${i}.ts`);
      helpers.setupExecSuccess(helpers.createLsOutput(files));
      helpers.setupLstatFile();

      const result = await viewStructure({
        path: '/workspace',
        itemsPerPage: 10,
        page: 1,
      });

      expect(result.status).toBeUndefined();
      expect(result.pagination).toBeDefined();
      expect(result.pagination?.currentPage).toBe(1);
      expect(result.pagination?.hasMore).toBe(true);
    });
  });

  describe('Research Goals Passthrough', () => {
    it('should not echo researchGoal and reasoning in results', async () => {
      helpers.setupExecSuccess('');

      const result = await viewStructure({
        path: '/workspace',
        researchGoal: 'Find configuration files',
        reasoning: 'Need to understand project setup',
      });

      expect(result).not.toHaveProperty('mainResearchGoal');
      expect(result).not.toHaveProperty('researchGoal');
      expect(result).not.toHaveProperty('reasoning');
    });
  });
});

describe('Example: Direct Mock Access', () => {
  beforeEach(() => {
    helpers.reset();
    helpers.setupValidPath('/workspace');
    helpers.setupRealpath('/workspace');
  });

  it('should allow direct mock manipulation', async () => {
    // Using mocks.safeExec directly for complex scenarios
    mocks.safeExec.mockResolvedValueOnce({
      success: true,
      code: 0,
      stdout: 'file1.ts\nfile2.ts',
      stderr: '',
    });

    // Using mocks.lstat directly
    mocks.lstat.mockResolvedValue(createStats({ isFile: true, size: 500 }));

    const result = await viewStructure({ path: '/workspace' });
    expect(result.status).toBeUndefined();

    // Verify mock was called
    expect(mocks.safeExec).toHaveBeenCalled();
  });

  it('should track mock call arguments', async () => {
    helpers.setupExecSuccess('');

    await viewStructure({ path: '/workspace' });

    // Check what command was executed
    expect(mocks.safeExec).toHaveBeenCalledWith(
      'ls',
      expect.arrayContaining(['/workspace'])
    );
  });
});
