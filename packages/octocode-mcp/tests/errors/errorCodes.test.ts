import { describe, it, expect } from 'vitest';
import { LOCAL_TOOL_ERROR_CODES } from '../../src/errors/localToolErrors.js';
import {
  ToolError,
  isToolError,
  toToolError,
} from '../../src/errors/ToolError.js';
import { ToolErrors } from '../../src/errors/errorFactories.js';
import {
  LOCAL_TOOL_ERROR_REGISTRY,
  LocalToolErrorCategory as ErrorCategory,
} from '../../src/errors/localToolErrors.js';

describe('Local Error Codes', () => {
  describe('LOCAL_TOOL_ERROR_CODES', () => {
    it('should have all required error codes defined', () => {
      expect(LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED).toBe(
        'pathValidationFailed'
      );
      expect(LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED).toBe(
        'fileAccessFailed'
      );
      expect(LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED).toBe('fileReadFailed');
      expect(LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE).toBe('fileTooLarge');
      expect(LOCAL_TOOL_ERROR_CODES.NO_MATCHES).toBe('noMatches');
      expect(LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE).toBe('outputTooLarge');
      expect(LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE).toBe(
        'commandNotAvailable'
      );
      expect(LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED).toBe(
        'commandExecutionFailed'
      );
      expect(LOCAL_TOOL_ERROR_CODES.COMMAND_TIMEOUT).toBe('commandTimeout');
      expect(LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED).toBe(
        'toolExecutionFailed'
      );
    });
  });

  describe('LOCAL_TOOL_ERROR_REGISTRY', () => {
    it('should have metadata for all error codes', () => {
      Object.values(LOCAL_TOOL_ERROR_CODES).forEach(code => {
        expect(LOCAL_TOOL_ERROR_REGISTRY[code]).toBeDefined();
        expect(LOCAL_TOOL_ERROR_REGISTRY[code].code).toBe(code);
        expect(LOCAL_TOOL_ERROR_REGISTRY[code].category).toBeDefined();
        expect(LOCAL_TOOL_ERROR_REGISTRY[code].description).toBeDefined();
        expect(LOCAL_TOOL_ERROR_REGISTRY[code].recoverability).toBeDefined();
      });
    });

    it('should categorize errors correctly', () => {
      expect(LOCAL_TOOL_ERROR_REGISTRY.pathValidationFailed.category).toBe(
        ErrorCategory.VALIDATION
      );
      expect(LOCAL_TOOL_ERROR_REGISTRY.fileAccessFailed.category).toBe(
        ErrorCategory.FILE_SYSTEM
      );
      expect(LOCAL_TOOL_ERROR_REGISTRY.noMatches.category).toBe(
        ErrorCategory.SEARCH
      );
      expect(LOCAL_TOOL_ERROR_REGISTRY.outputTooLarge.category).toBe(
        ErrorCategory.PAGINATION
      );
      expect(LOCAL_TOOL_ERROR_REGISTRY.commandExecutionFailed.category).toBe(
        ErrorCategory.EXECUTION
      );
    });
  });

  describe('ToolError', () => {
    it('should create error with all properties', () => {
      const error = new ToolError(
        LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED,
        'Cannot access file',
        { path: '/test/file.txt' }
      );

      expect(error.errorCode).toBe('fileAccessFailed');
      expect(error.message).toBe('Cannot access file');
      expect(error.context).toEqual({ path: '/test/file.txt' });
      expect(error.category).toBe(ErrorCategory.FILE_SYSTEM);
      expect(error.recoverability).toBe('unrecoverable');
    });

    it('should include cause in stack trace', () => {
      const cause = new Error('Original error');
      const error = new ToolError(
        LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED,
        'Read failed',
        undefined,
        cause
      );

      expect(error.stack).toContain('Caused by:');
      expect(error.stack).toContain('Original error');
    });

    it('should check recoverability correctly', () => {
      const unrecoverable = new ToolError(
        LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED,
        'Access failed'
      );
      const userAction = new ToolError(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED,
        'Invalid path'
      );
      const noMatches = new ToolError(
        LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
        'No matches'
      );

      expect(unrecoverable.isRecoverable()).toBe(false);
      expect(unrecoverable.requiresUserAction()).toBe(false);
      expect(userAction.requiresUserAction()).toBe(true);
      expect(userAction.isRecoverable()).toBe(false);
      expect(noMatches.requiresUserAction()).toBe(true);
    });

    it('should serialize to JSON correctly', () => {
      const error = new ToolError(
        LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE,
        'File too large',
        { sizeKB: 1000, limitKB: 500 }
      );

      const json = error.toJSON();

      expect(json.name).toBe('ToolError');
      expect(json.errorCode).toBe('fileTooLarge');
      expect(json.category).toBe(ErrorCategory.FILE_SYSTEM);
      expect(json.message).toBe('File too large');
      expect(json.recoverability).toBe('user-action-required');
      expect(json.context).toEqual({ sizeKB: 1000, limitKB: 500 });
      expect(json.stack).toBeDefined();
    });
  });

  describe('isToolError', () => {
    it('should return true for ToolError instances', () => {
      const error = new ToolError(LOCAL_TOOL_ERROR_CODES.NO_MATCHES, 'Test');
      expect(isToolError(error)).toBe(true);
    });

    it('should return false for regular errors', () => {
      const error = new Error('Test');
      expect(isToolError(error)).toBe(false);
    });

    it('should return false for non-error values', () => {
      expect(isToolError(null)).toBe(false);
      expect(isToolError(undefined)).toBe(false);
      expect(isToolError('error')).toBe(false);
      expect(isToolError({ message: 'error' })).toBe(false);
    });
  });

  describe('toToolError', () => {
    it('should return same error if already ToolError', () => {
      const original = new ToolError(LOCAL_TOOL_ERROR_CODES.NO_MATCHES, 'Test');
      const result = toToolError(original);
      expect(result).toBe(original);
    });

    it('should convert Error to ToolError', () => {
      const original = new Error('Original message');
      const result = toToolError(original);

      expect(result).toBeInstanceOf(ToolError);
      expect(result.message).toBe('Original message');
      expect(result.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED
      );
    });

    it('should use custom error code when converting Error', () => {
      const original = new Error('Read error');
      const result = toToolError(
        original,
        LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED
      );

      expect(result.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED);
    });

    it('should convert string to ToolError', () => {
      const result = toToolError('String error');

      expect(result).toBeInstanceOf(ToolError);
      expect(result.message).toBe('String error');
    });

    it('should convert other types to ToolError', () => {
      const result = toToolError({ custom: 'object' });

      expect(result).toBeInstanceOf(ToolError);
      expect(result.message).toContain('object');
    });

    it('should include context when converting', () => {
      const original = new Error('Test');
      const result = toToolError(
        original,
        LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED,
        {
          path: '/test',
        }
      );

      expect(result.context).toEqual({ path: '/test' });
    });
  });

  describe('ToolErrors factory functions', () => {
    it('should create pathValidationFailed error', () => {
      const error = ToolErrors.pathValidationFailed(
        '/invalid/path',
        'Outside workspace'
      );

      expect(error.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
      expect(error.message).toBe('Outside workspace');
      expect(error.context).toEqual({ path: '/invalid/path' });
    });

    it('should create pathValidationFailed error with default message', () => {
      const error = ToolErrors.pathValidationFailed('/invalid/path');

      expect(error.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED
      );
      expect(error.message).toContain('path');
    });

    it('should create fileAccessFailed error', () => {
      const cause = new Error('ENOENT');
      const error = ToolErrors.fileAccessFailed('/missing/file.txt', cause);

      expect(error.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED);
      expect(error.message).toContain('file.txt');
      expect(error.context).toEqual({
        path: '/missing/file.txt',
        errorCode: undefined,
      });
      expect(error.stack).toContain('Caused by:');
    });

    it('should create fileAccessFailed error with ENOENT code', () => {
      const cause = Object.assign(new Error('File not found'), {
        code: 'ENOENT',
      });
      const error = ToolErrors.fileAccessFailed('/missing/file.txt', cause);

      expect(error.message).toContain('File not found');
      expect(error.message).toContain('Verify the path exists');
      expect(error.context).toEqual({
        path: '/missing/file.txt',
        errorCode: 'ENOENT',
      });
    });

    it('should create fileAccessFailed error with EACCES code', () => {
      const cause = Object.assign(new Error('Permission denied'), {
        code: 'EACCES',
      });
      const error = ToolErrors.fileAccessFailed('/protected/file.txt', cause);

      expect(error.message).toContain('Permission denied');
      expect(error.message).toContain('Check file permissions');
      expect(error.context).toEqual({
        path: '/protected/file.txt',
        errorCode: 'EACCES',
      });
    });

    it('should create fileAccessFailed error with EISDIR code', () => {
      const cause = Object.assign(new Error('Is a directory'), {
        code: 'EISDIR',
      });
      const error = ToolErrors.fileAccessFailed('/some/directory', cause);

      expect(error.message).toContain('Path is a directory');
      expect(error.message).toContain('localViewStructure');
      expect(error.context).toEqual({
        path: '/some/directory',
        errorCode: 'EISDIR',
      });
    });

    it('should create fileAccessFailed error with ENOTDIR code', () => {
      const cause = Object.assign(new Error('Not a directory'), {
        code: 'ENOTDIR',
      });
      const error = ToolErrors.fileAccessFailed('/file.txt/child', cause);

      expect(error.message).toContain('Invalid path');
      expect(error.message).toContain('component of the path');
      expect(error.context).toEqual({
        path: '/file.txt/child',
        errorCode: 'ENOTDIR',
      });
    });

    it('should create fileAccessFailed error with ENAMETOOLONG code', () => {
      const longPath = '/a'.repeat(500);
      const cause = Object.assign(new Error('Name too long'), {
        code: 'ENAMETOOLONG',
      });
      const error = ToolErrors.fileAccessFailed(longPath, cause);

      expect(error.message).toContain('Path too long');
      expect(error.context).toEqual({
        path: longPath,
        errorCode: 'ENAMETOOLONG',
      });
    });

    it('should create fileAccessFailed error with unknown error code', () => {
      const cause = Object.assign(new Error('Unknown error'), {
        code: 'UNKNOWN',
      });
      const error = ToolErrors.fileAccessFailed('/some/path', cause);

      expect(error.message).toContain('Cannot access file');
      expect(error.context).toEqual({
        path: '/some/path',
        errorCode: 'UNKNOWN',
      });
    });

    it('should create fileReadFailed error', () => {
      const cause = new Error('Read error');
      const error = ToolErrors.fileReadFailed('/test/file.txt', cause);

      expect(error.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED);
      expect(error.message).toContain('file.txt');
      expect(error.stack).toContain('Caused by:');
    });

    it('should create fileTooLarge error with formatted sizes', () => {
      const error = ToolErrors.fileTooLarge('/big/file.bin', 1000, 500);

      expect(error.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE);
      expect(error.message).toContain('1000KB');
      expect(error.message).toContain('500KB');
      expect(error.context).toEqual({
        path: '/big/file.bin',
        sizeKB: 1000,
        limitKB: 500,
      });
    });

    it('should create fileTooLarge error with decimal sizes', () => {
      const error = ToolErrors.fileTooLarge('/big/file.bin', 1000.5, 500.25);

      expect(error.message).toContain('1000.5KB');
      expect(error.message).toContain('500.3KB');
    });

    it('should create outputTooLarge error', () => {
      const error = ToolErrors.outputTooLarge(100000, 50000);

      expect(error.errorCode).toBe(LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE);
      expect(error.message).toContain('100000');
      expect(error.message).toContain('50000');
      expect(error.context).toEqual({ size: 100000, limit: 50000 });
    });

    it('should create commandNotAvailable error', () => {
      const error = ToolErrors.commandNotAvailable('rg', 'Install ripgrep');

      expect(error.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE
      );
      expect(error.message).toContain('rg');
      expect(error.message).toContain('Install ripgrep');
    });

    it('should create commandExecutionFailed error', () => {
      const cause = new Error('Timeout');
      const error = ToolErrors.commandExecutionFailed('find /path', cause);

      expect(error.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED
      );
      expect(error.message).toContain('find /path');
      expect(error.context).toEqual({
        command: 'find /path',
        stderr: undefined,
      });
      expect(error.stack).toContain('Caused by:');
    });

    it('should create toolExecutionFailed error', () => {
      const cause = new Error('Internal error');
      const error = ToolErrors.toolExecutionFailed('localSearchCode', cause);

      expect(error.errorCode).toBe(
        LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED
      );
      expect(error.message).toContain('localSearchCode');
      expect(error.context).toEqual({ toolName: 'localSearchCode' });
      expect(error.stack).toContain('Caused by:');
    });
  });
});
