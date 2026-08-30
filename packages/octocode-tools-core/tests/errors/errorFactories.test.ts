import { describe, expect, it } from 'vitest';
import { ToolErrors } from '../../src/errors/errorFactories.js';
import { ToolError } from '../../src/errors/ToolError.js';

describe('ToolErrors.pathValidationFailed', () => {
  it('creates a ToolError with the provided reason', () => {
    const err = ToolErrors.pathValidationFailed(
      '/etc/passwd',
      'outside workspace'
    );
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toContain('outside workspace');
  });

  it('uses a default message when no reason is provided', () => {
    const err = ToolErrors.pathValidationFailed('/some/path');
    expect(err.message).toMatch(/path validation failed/i);
  });

  it('includes the file path in the context', () => {
    const err = ToolErrors.pathValidationFailed('/some/path');
    expect(
      (err as ToolError & { context: { path: string } }).context?.path
    ).toBe('/some/path');
  });
});

describe('ToolErrors.fileAccessFailed', () => {
  it('creates a generic access error when no cause', () => {
    const err = ToolErrors.fileAccessFailed('/missing');
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toMatch(/cannot access file/i);
  });

  it('produces ENOENT message for file-not-found cause', () => {
    const cause = Object.assign(new Error('no such file'), { code: 'ENOENT' });
    const err = ToolErrors.fileAccessFailed('/path/file.ts', cause);
    expect(err.message).toMatch(/file not found/i);
    expect(err.message).toMatch(/localFindFiles/i);
  });

  it('produces EACCES message for permission-denied cause', () => {
    const cause = Object.assign(new Error('permission denied'), {
      code: 'EACCES',
    });
    const err = ToolErrors.fileAccessFailed('/path/file.ts', cause);
    expect(err.message).toMatch(/permission denied/i);
  });

  it('produces EISDIR message for is-a-directory cause', () => {
    const cause = Object.assign(new Error('is a directory'), {
      code: 'EISDIR',
    });
    const err = ToolErrors.fileAccessFailed('/path/dir', cause);
    expect(err.message).toMatch(/path is a directory/i);
    expect(err.message).toMatch(/localViewStructure/i);
  });

  it('produces ENOTDIR message', () => {
    const cause = Object.assign(new Error('not a directory'), {
      code: 'ENOTDIR',
    });
    const err = ToolErrors.fileAccessFailed('/a/b/c', cause);
    expect(err.message).toMatch(/invalid path/i);
  });

  it('produces ENAMETOOLONG message', () => {
    const cause = Object.assign(new Error('name too long'), {
      code: 'ENAMETOOLONG',
    });
    const err = ToolErrors.fileAccessFailed('/a/b/c', cause);
    expect(err.message).toMatch(/path too long/i);
  });

  it('uses a generic message for unknown error codes', () => {
    const cause = Object.assign(new Error('unknown'), { code: 'EUNKNOWN' });
    const err = ToolErrors.fileAccessFailed('/a/b/c', cause);
    expect(err.message).toMatch(/cannot access file/i);
  });
});

describe('ToolErrors.fileReadFailed', () => {
  it('creates an error with the file path in the message', () => {
    const err = ToolErrors.fileReadFailed('/my/file.ts');
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toMatch(/failed to read file/i);
  });

  it('accepts an optional cause with a code', () => {
    const cause = Object.assign(new Error('disk error'), { code: 'EIO' });
    const err = ToolErrors.fileReadFailed('/file.ts', cause);
    expect(err).toBeInstanceOf(ToolError);
  });
});

describe('ToolErrors.fileTooLarge', () => {
  it('formats integer KB values without decimals', () => {
    const err = ToolErrors.fileTooLarge('/big.ts', 500, 256);
    expect(err.message).toContain('500KB');
    expect(err.message).toContain('256KB');
    expect(err.message).not.toMatch(/\d+\.\d+KB/);
  });

  it('formats fractional KB values with one decimal place', () => {
    const err = ToolErrors.fileTooLarge('/big.ts', 1.5, 1.0);
    expect(err.message).toContain('1.5KB');
  });

  it('mentions charOffset/charLength in the message', () => {
    const err = ToolErrors.fileTooLarge('/big.ts', 100, 50);
    expect(err.message).toMatch(/charOffset|charLength/i);
  });
});

describe('ToolErrors.binaryFileUnsupported', () => {
  it('creates a ToolError mentioning binary file', () => {
    const err = ToolErrors.binaryFileUnsupported('/image.png');
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toMatch(/binary file/i);
    expect(err.message).toMatch(/localSearchCode/i);
  });
});

describe('ToolErrors.outputTooLarge', () => {
  it('creates an error with size and limit', () => {
    const err = ToolErrors.outputTooLarge(200000, 100000);
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toContain('200000');
    expect(err.message).toContain('100000');
  });
});

describe('ToolErrors.commandNotAvailable', () => {
  it('creates an error with the command name', () => {
    const err = ToolErrors.commandNotAvailable('ripgrep');
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toContain('ripgrep');
  });

  it('includes install hint when provided', () => {
    const err = ToolErrors.commandNotAvailable(
      'rg',
      'Install via brew install ripgrep'
    );
    expect(err.message).toContain('Install via brew install ripgrep');
  });

  it('uses a default hint when installHint is omitted', () => {
    const err = ToolErrors.commandNotAvailable('git');
    expect(err.message).toMatch(/PATH/i);
  });
});

describe('ToolErrors.commandExecutionFailed', () => {
  it('uses stderr in the message when provided', () => {
    const err = ToolErrors.commandExecutionFailed(
      'rg',
      undefined,
      'regex syntax error'
    );
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toContain('regex syntax error');
  });

  it('uses a generic message when no stderr', () => {
    const err = ToolErrors.commandExecutionFailed('rg');
    expect(err.message).toMatch(/command execution failed/i);
  });

  it('accepts an optional cause', () => {
    const cause = new Error('underlying cause');
    const err = ToolErrors.commandExecutionFailed('rg', cause);
    expect(err).toBeInstanceOf(ToolError);
  });
});

describe('ToolErrors.toolExecutionFailed', () => {
  it('creates an error mentioning the tool name', () => {
    const err = ToolErrors.toolExecutionFailed('localSearchCode');
    expect(err).toBeInstanceOf(ToolError);
    expect(err.message).toContain('localSearchCode');
  });

  it('accepts an optional cause', () => {
    const err = ToolErrors.toolExecutionFailed(
      'localSearchCode',
      new Error('cause')
    );
    expect(err).toBeInstanceOf(ToolError);
  });
});
