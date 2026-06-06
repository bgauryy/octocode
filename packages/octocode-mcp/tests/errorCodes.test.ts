import os from 'os';
import { describe, it, expect, vi } from 'vitest';
import { getConfigSync } from 'octocode-shared';
import {
  CONFIG_ERRORS,
  VALIDATION_ERRORS,
  FETCH_ERRORS,
  TOOL_METADATA_ERRORS,
  FILE_OPERATION_ERRORS,
  REPOSITORY_ERRORS,
  SEARCH_ERRORS,
  STARTUP_ERRORS,
  PROMISE_ERRORS,
  TOOL_ERRORS,
} from '../src/errors/domainErrors.js';
import { ALL_ERROR_CODES } from '../src/errors/domainErrors.js';
import { redactPath } from '../src/errors/pathUtils.js';

describe('errorCodes', () => {
  describe('Error Constants', () => {
    describe('CONFIG_ERRORS', () => {
      it('should have NOT_INITIALIZED error', () => {
        expect(CONFIG_ERRORS.NOT_INITIALIZED).toMatchObject({
          code: 'CONFIG_NOT_INITIALIZED',
          message: expect.stringContaining('initialize()'),
        });
      });
    });

    describe('VALIDATION_ERRORS', () => {
      it('should have PROMISES_NOT_ARRAY error', () => {
        expect(VALIDATION_ERRORS.PROMISES_NOT_ARRAY).toMatchObject({
          code: 'VALIDATION_PROMISES_NOT_ARRAY',
          message: 'promises must be an array',
        });
      });

      it('should have TIMEOUT_NOT_POSITIVE error', () => {
        expect(VALIDATION_ERRORS.TIMEOUT_NOT_POSITIVE).toMatchObject({
          code: 'VALIDATION_TIMEOUT_NOT_POSITIVE',
          message: 'timeout must be positive',
        });
      });

      it('should have CONCURRENCY_NOT_POSITIVE error', () => {
        expect(VALIDATION_ERRORS.CONCURRENCY_NOT_POSITIVE).toMatchObject({
          code: 'VALIDATION_CONCURRENCY_NOT_POSITIVE',
          message: 'concurrency must be positive',
        });
      });
    });

    describe('FETCH_ERRORS', () => {
      it('should have FETCH_NOT_AVAILABLE error', () => {
        expect(FETCH_ERRORS.FETCH_NOT_AVAILABLE).toMatchObject({
          code: 'FETCH_NOT_AVAILABLE',
          message: expect.stringContaining('fetch'),
        });
      });

      it('should have FETCH_FAILED_AFTER_RETRIES error with function message', () => {
        const error = FETCH_ERRORS.FETCH_FAILED_AFTER_RETRIES;
        expect(error.code).toBe('FETCH_FAILED_AFTER_RETRIES');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message(3, 'Network error');
        expect(msg).toContain('3');
        expect(msg).toContain('Network error');
      });

      it('should have FETCH_HTTP_ERROR error with function message', () => {
        const error = FETCH_ERRORS.FETCH_HTTP_ERROR;
        expect(error.code).toBe('FETCH_HTTP_ERROR');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message(404, 'Not Found');
        expect(msg).toContain('404');
        expect(msg).toContain('Not Found');
      });
    });

    describe('TOOL_METADATA_ERRORS', () => {
      it('should have INVALID_FORMAT error', () => {
        expect(TOOL_METADATA_ERRORS.INVALID_FORMAT).toMatchObject({
          code: 'TOOL_METADATA_INVALID_FORMAT',
          message: expect.stringContaining('format'),
        });
      });

      it('should have INVALID_API_RESPONSE error', () => {
        expect(TOOL_METADATA_ERRORS.INVALID_API_RESPONSE).toMatchObject({
          code: 'TOOL_METADATA_INVALID_API_RESPONSE',
          message: expect.stringContaining('API response'),
        });
      });
    });

    describe('FILE_OPERATION_ERRORS', () => {
      it('should have PATH_IS_DIRECTORY error with function message', () => {
        const error = FILE_OPERATION_ERRORS.PATH_IS_DIRECTORY;
        expect(error.code).toBe('FILE_PATH_IS_DIRECTORY');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('githubViewRepoStructure')).toContain(
          'githubViewRepoStructure'
        );
      });

      it('should have FILE_TOO_LARGE error with function message', () => {
        const error = FILE_OPERATION_ERRORS.FILE_TOO_LARGE;
        expect(error.code).toBe('FILE_TOO_LARGE');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message(1000, 500, 'githubSearchCode');
        expect(msg).toContain('1000KB');
        expect(msg).toContain('500KB');
        expect(msg).toContain('githubSearchCode');
      });

      it('should have FILE_EMPTY error', () => {
        expect(FILE_OPERATION_ERRORS.FILE_EMPTY).toMatchObject({
          code: 'FILE_EMPTY',
          message: expect.stringContaining('empty'),
        });
      });

      it('should have BINARY_FILE error', () => {
        expect(FILE_OPERATION_ERRORS.BINARY_FILE).toMatchObject({
          code: 'FILE_BINARY',
          message: expect.stringContaining('Binary'),
        });
      });

      it('should have DECODE_FAILED error', () => {
        expect(FILE_OPERATION_ERRORS.DECODE_FAILED).toMatchObject({
          code: 'FILE_DECODE_FAILED',
          message: expect.stringContaining('decode'),
        });
      });

      it('should have UNSUPPORTED_TYPE error with function message', () => {
        const error = FILE_OPERATION_ERRORS.UNSUPPORTED_TYPE;
        expect(error.code).toBe('FILE_UNSUPPORTED_TYPE');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('submodule')).toContain('submodule');
      });
    });

    describe('REPOSITORY_ERRORS', () => {
      it('should have NOT_FOUND error with function message', () => {
        const error = REPOSITORY_ERRORS.NOT_FOUND;
        expect(error.code).toBe('REPO_NOT_FOUND');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('owner', 'repo', 'Access denied');
        expect(msg).toContain('owner');
        expect(msg).toContain('repo');
        expect(msg).toContain('Access denied');
      });

      it('should have PATH_NOT_FOUND error with function message', () => {
        const error = REPOSITORY_ERRORS.PATH_NOT_FOUND;
        expect(error.code).toBe('REPO_PATH_NOT_FOUND');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('src/file.ts', 'owner', 'repo', 'main');
        expect(msg).toContain('src/file.ts');
        expect(msg).toContain('owner');
        expect(msg).toContain('repo');
        expect(msg).toContain('main');
      });

      it('should have PATH_NOT_FOUND_ANY_BRANCH error with function message', () => {
        const error = REPOSITORY_ERRORS.PATH_NOT_FOUND_ANY_BRANCH;
        expect(error.code).toBe('REPO_PATH_NOT_FOUND_ANY_BRANCH');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('src/file.ts', 'owner', 'repo');
        expect(msg).toContain('src/file.ts');
        expect(msg).toContain('owner');
        expect(msg).toContain('repo');
      });

      it('should have ACCESS_FAILED error with function message', () => {
        const error = REPOSITORY_ERRORS.ACCESS_FAILED;
        expect(error.code).toBe('REPO_ACCESS_FAILED');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('owner', 'repo', 'Network error');
        expect(msg).toContain('owner');
        expect(msg).toContain('repo');
        expect(msg).toContain('Network error');
      });

      it('should have STRUCTURE_EXPLORATION_FAILED error', () => {
        expect(REPOSITORY_ERRORS.STRUCTURE_EXPLORATION_FAILED).toMatchObject({
          code: 'REPO_STRUCTURE_EXPLORATION_FAILED',
          message: expect.stringContaining('structure'),
        });
      });
    });

    describe('SEARCH_ERRORS', () => {
      it('should have QUERY_EMPTY error', () => {
        expect(SEARCH_ERRORS.QUERY_EMPTY).toMatchObject({
          code: 'SEARCH_QUERY_EMPTY',
          message: expect.stringContaining('empty'),
        });
      });

      it('should have NO_VALID_PARAMETERS error', () => {
        expect(SEARCH_ERRORS.NO_VALID_PARAMETERS).toMatchObject({
          code: 'SEARCH_NO_VALID_PARAMETERS',
          message: expect.stringContaining('parameters'),
        });
      });

      it('should have PR_REQUIRED_PARAMS error', () => {
        expect(SEARCH_ERRORS.PR_REQUIRED_PARAMS).toMatchObject({
          code: 'SEARCH_PR_REQUIRED_PARAMS',
          message: expect.stringContaining('required'),
        });
      });

      it('should have PR_SINGLE_VALUES error', () => {
        expect(SEARCH_ERRORS.PR_SINGLE_VALUES).toMatchObject({
          code: 'SEARCH_PR_SINGLE_VALUES',
          message: expect.stringContaining('single values'),
        });
      });

      it('should have PULL_REQUEST_SEARCH_FAILED error with function message', () => {
        const error = SEARCH_ERRORS.PULL_REQUEST_SEARCH_FAILED;
        expect(error.code).toBe('SEARCH_PR_SEARCH_FAILED');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('Invalid query')).toContain('Invalid query');
      });

      it('should have PULL_REQUEST_LIST_FAILED error with function message', () => {
        const error = SEARCH_ERRORS.PULL_REQUEST_LIST_FAILED;
        expect(error.code).toBe('SEARCH_PR_LIST_FAILED');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('API error')).toContain('API error');
      });

      it('should have PULL_REQUEST_FETCH_FAILED error with function message', () => {
        const error = SEARCH_ERRORS.PULL_REQUEST_FETCH_FAILED;
        expect(error.code).toBe('SEARCH_PR_FETCH_FAILED');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message(123, 'Not found');
        expect(msg).toContain('123');
        expect(msg).toContain('Not found');
      });
    });

    describe('STARTUP_ERRORS', () => {
      it('should have NO_TOOLS_REGISTERED error', () => {
        expect(STARTUP_ERRORS.NO_TOOLS_REGISTERED).toMatchObject({
          code: 'STARTUP_NO_TOOLS_REGISTERED',
          message: expect.stringContaining('tools'),
        });
      });

      it('should have UNCAUGHT_EXCEPTION error with function message', () => {
        const error = STARTUP_ERRORS.UNCAUGHT_EXCEPTION;
        expect(error.code).toBe('STARTUP_UNCAUGHT_EXCEPTION');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('Division by zero')).toContain('Division by zero');
      });

      it('should have UNHANDLED_REJECTION error with function message', () => {
        const error = STARTUP_ERRORS.UNHANDLED_REJECTION;
        expect(error.code).toBe('STARTUP_UNHANDLED_REJECTION');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('Promise rejected')).toContain('Promise rejected');
      });

      it('should have STARTUP_FAILED error with function message', () => {
        const error = STARTUP_ERRORS.STARTUP_FAILED;
        expect(error.code).toBe('STARTUP_FAILED');
        expect(error.message).toBeTypeOf('function');
        expect(error.message('Init failed')).toContain('Init failed');
      });
    });

    describe('PROMISE_ERRORS', () => {
      it('should have TIMEOUT error with function message', () => {
        const error = PROMISE_ERRORS.TIMEOUT;
        expect(error.code).toBe('PROMISE_TIMEOUT');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message(0, 5000);
        expect(msg).toContain('0');
        expect(msg).toContain('5000ms');
      });

      it('should have NOT_A_FUNCTION error with function message', () => {
        const error = PROMISE_ERRORS.NOT_A_FUNCTION;
        expect(error.code).toBe('PROMISE_NOT_A_FUNCTION');
        expect(error.message).toBeTypeOf('function');
        expect(error.message(2)).toContain('2');
      });

      it('should have FUNCTION_UNDEFINED error', () => {
        expect(PROMISE_ERRORS.FUNCTION_UNDEFINED).toMatchObject({
          code: 'PROMISE_FUNCTION_UNDEFINED',
          message: expect.stringContaining('undefined'),
        });
      });
    });

    describe('ALL_ERROR_CODES', () => {
      it('should contain all error codes from all categories', () => {
        expect(ALL_ERROR_CODES).toMatchObject({
          ...CONFIG_ERRORS,
          ...VALIDATION_ERRORS,
          ...FETCH_ERRORS,
          ...TOOL_METADATA_ERRORS,
          ...FILE_OPERATION_ERRORS,
          ...REPOSITORY_ERRORS,
          ...SEARCH_ERRORS,
          ...STARTUP_ERRORS,
          ...PROMISE_ERRORS,
        });
      });

      it('should have unique error codes', () => {
        const codes = Object.values(ALL_ERROR_CODES).map(error =>
          typeof error.code === 'string' ? error.code : 'INVALID'
        );
        const uniqueCodes = new Set(codes);
        expect(codes.length).toBe(uniqueCodes.size);
      });
    });

    describe('TOOL_ERRORS', () => {
      it('should have EXECUTION_FAILED error with function message', () => {
        const error = TOOL_ERRORS.EXECUTION_FAILED;
        expect(error.code).toBe('TOOL_EXECUTION_FAILED');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('myTool', 'Something went wrong');
        expect(msg).toContain('myTool');
        expect(msg).toContain('Something went wrong');
      });

      it('should have SECURITY_VALIDATION_FAILED error with function message', () => {
        const error = TOOL_ERRORS.SECURITY_VALIDATION_FAILED;
        expect(error.code).toBe('TOOL_SECURITY_VALIDATION_FAILED');
        expect(error.message).toBeTypeOf('function');
        const msg = error.message('myTool', 'Invalid input');
        expect(msg).toContain('myTool');
        expect(msg).toContain('Invalid input');
      });
    });
  });

  describe('redactPath function', () => {
    it.each([
      ['unix source file', '/project/src/file.ts', '/project', 'src/file.ts'],
      ['unix .env', '/project/.env', '/project', '.env'],
      [
        'unix .env.production',
        '/project/.env.production',
        '/project',
        '.env.production',
      ],
      [
        'unix nested secrets',
        '/project/config/secrets/key.json',
        '/project',
        'config/secrets/key.json',
      ],
      [
        'unix deeply nested',
        '/project/a/b/c/d/e/file.ts',
        '/project',
        'a/b/c/d/e/file.ts',
      ],
      ['unix workspace root itself', '/project', '/project', '.'],
      [
        'unix trailing slash on root',
        '/project/src/file.ts',
        '/project/',
        'src/file.ts',
      ],
      [
        'unix double slashes',
        '/project//src//file.ts',
        '/project',
        'src/file.ts',
      ],
      [
        'win backslash paths',
        'C:\\Users\\dev\\project\\src\\file.ts',
        'C:\\Users\\dev\\project',
        'src/file.ts',
      ],
      [
        'win .env',
        'C:\\Users\\dev\\project\\.env',
        'C:\\Users\\dev\\project',
        '.env',
      ],
      [
        'win nested config',
        'C:\\Users\\dev\\project\\config\\secrets\\key.json',
        'C:\\Users\\dev\\project',
        'config/secrets/api-key.json'.replace('api-key.json', 'key.json'),
      ],
      [
        'win mixed slashes',
        'C:\\Users\\dev\\project/src\\config/.env.local',
        'C:\\Users\\dev\\project',
        'src/config/.env.local',
      ],
    ])('%s → %s', (_desc, absolutePath, workspace, expected) => {
      expect(redactPath(absolutePath, workspace)).toBe(expected);
    });

    it.each([
      [
        '/workspace/project-secret/creds.json',
        '/workspace/project',
        'creds.json',
      ],
      ['/home/user-backup/secret.key', '/home/user', 'secret.key'],
      [
        'C:\\Users\\dev\\project-secret\\creds.json',
        'C:\\Users\\dev\\project',
        'creds.json',
      ],
    ])(
      'prefix collision: %s with root %s → %s',
      (absolutePath, workspace, expected) => {
        expect(redactPath(absolutePath, workspace)).toBe(expected);
      }
    );

    it.each([
      ['/project/src/../../../etc/passwd', '/project', 'passwd'],
      ['C:\\project\\..\\..\\Windows\\System32\\SAM', 'C:\\project', 'SAM'],
    ])('traversal: %s → %s', (absolutePath, workspace, expected) => {
      expect(redactPath(absolutePath, workspace)).toBe(expected);
    });

    it('home directory fallback → ~/...', () => {
      const homeDir = os.homedir();
      expect(redactPath(`${homeDir}/documents/file.ts`)).toBe(
        '~/documents/file.ts'
      );
    });

    it('home directory exact match → ~', () => {
      const homeDir = os.homedir();
      expect(redactPath(homeDir, '/unrelated/workspace')).toBe('~');
    });

    it('outside all roots fallback → filename only', () => {
      expect(redactPath('/var/log/app/error.log', '/unrelated/workspace')).toBe(
        'error.log'
      );
    });

    it('UNC path outside workspace → filename only', () => {
      expect(
        redactPath('\\\\server\\share\\secrets\\key.pem', 'C:\\project')
      ).toBe('key.pem');
    });

    it('empty path → empty string', () => {
      expect(redactPath('')).toBe('');
    });

    it('auto-resolves workspaceRoot from CWD when not provided', () => {
      const cwd = process.cwd();
      expect(redactPath(`${cwd}/src/test-file.ts`)).toBe('src/test-file.ts');
    });

    it('ignores missing WORKSPACE_ROOT and preserves normal fallback behavior', () => {
      const originalWorkspaceRoot = process.env.WORKSPACE_ROOT;
      const cwd = process.cwd();
      try {
        process.env.WORKSPACE_ROOT = `${cwd}/__missing_workspace_root__`;
        expect(redactPath(`${cwd}/src/test-file.ts`)).toBe('src/test-file.ts');
      } finally {
        if (originalWorkspaceRoot) {
          process.env.WORKSPACE_ROOT = originalWorkspaceRoot;
        } else {
          delete process.env.WORKSPACE_ROOT;
        }
      }
    });

    it('auto-resolves for nested paths without explicit workspace', () => {
      const cwd = process.cwd();
      expect(redactPath(`${cwd}/packages/octocode-mcp/src/errorCodes.ts`)).toBe(
        'packages/octocode-mcp/src/errorCodes.ts'
      );
    });

    it('auto-resolves workspaceRoot from config when set', () => {
      const mockedGetConfig = vi.mocked(getConfigSync);
      const cwd = process.cwd();
      mockedGetConfig.mockReturnValueOnce({
        ...mockedGetConfig(),
        local: {
          enabled: true,
          enableClone: false,
          allowedPaths: [],
          workspaceRoot: cwd,
        },
      });
      expect(redactPath(`${cwd}/src/file.ts`)).toBe('src/file.ts');
    });
  });
});
