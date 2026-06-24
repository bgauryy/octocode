import { ToolError } from './ToolError.js';
import { LOCAL_TOOL_ERROR_CODES } from './localToolErrors.js';
import { redactPath } from './pathUtils.js';

export const ToolErrors = {
  pathValidationFailed: (
    filePath: string,
    reason?: string,
    workspaceRoot?: string
  ) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED,
      reason ||
        `Path validation failed: ${redactPath(filePath, workspaceRoot)}`,
      { path: filePath }
    ),

  fileAccessFailed: (
    filePath: string,
    cause?: Error,
    workspaceRoot?: string
  ) => {
    const displayPath = redactPath(filePath, workspaceRoot);
    let message = `Cannot access file: ${displayPath}`;
    const errorCode = (cause as Error & { code?: string })?.code;

    if (errorCode === 'ENOENT') {
      message = `File not found: ${displayPath}. Verify the path exists using localFindFiles.`;
    } else if (errorCode === 'EACCES') {
      message = `Permission denied: ${displayPath}. Check file permissions.`;
    } else if (errorCode === 'EISDIR') {
      message = `Path is a directory: ${displayPath}. Use localViewStructure instead.`;
    } else if (errorCode === 'ENOTDIR') {
      message = `Invalid path: ${displayPath}. A component of the path is not a directory.`;
    } else if (errorCode === 'ENAMETOOLONG') {
      message = `Path too long: ${displayPath}`;
    }

    return new ToolError(
      LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED,
      message,
      { path: filePath, errorCode },
      cause
    );
  },

  fileReadFailed: (filePath: string, cause?: Error, workspaceRoot?: string) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED,
      `Failed to read file: ${redactPath(filePath, workspaceRoot)}`,
      {
        path: filePath,
        errorCode: (cause as (Error & { code?: string }) | undefined)?.code,
      },
      cause
    ),

  fileTooLarge: (filePath: string, sizeKB: number, limitKB: number) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE,
      (() => {
        const fmt = (n: number) =>
          Number.isInteger(n) ? `${n}KB` : `${n.toFixed(1)}KB`;
        return `File too large: ${fmt(sizeKB)} (limit: ${fmt(limitKB)})`;
      })(),
      { path: filePath, sizeKB, limitKB }
    ),

  binaryFileUnsupported: (filePath: string) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.BINARY_FILE_UNSUPPORTED,
      `Binary file unsupported: ${redactPath(filePath)}. Use localBinaryInspect to read its format/strings, or localSearchCode to grep embedded strings.`,
      { path: filePath }
    ),

  outputTooLarge: (size: number, limit: number) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE,
      `Output too large: ${size} (limit: ${limit})`,
      { size, limit }
    ),

  commandNotAvailable: (command: string, installHint?: string) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE,
      `Command '${command}' is not available. ${installHint || 'Please install it and ensure it is in your PATH.'}`,
      { command, installHint }
    ),

  commandExecutionFailed: (command: string, cause?: Error, stderr?: string) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED,
      stderr
        ? `Command '${command}' failed: ${stderr}`
        : `Command execution failed: ${command}`,
      { command, stderr },
      cause
    ),

  toolExecutionFailed: (toolName: string, cause?: Error) =>
    new ToolError(
      LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED,
      `Tool execution failed: ${toolName}`,
      { toolName },
      cause
    ),
};
