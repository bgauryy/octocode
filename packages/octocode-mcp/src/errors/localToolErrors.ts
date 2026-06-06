export const LOCAL_TOOL_ERROR_CODES = {
  PATH_VALIDATION_FAILED: 'pathValidationFailed',
  FILE_ACCESS_FAILED: 'fileAccessFailed',
  FILE_READ_FAILED: 'fileReadFailed',
  FILE_TOO_LARGE: 'fileTooLarge',
  BINARY_FILE_UNSUPPORTED: 'binaryFileUnsupported',

  NO_MATCHES: 'noMatches',

  OUTPUT_TOO_LARGE: 'outputTooLarge',

  COMMAND_NOT_AVAILABLE: 'commandNotAvailable',
  COMMAND_EXECUTION_FAILED: 'commandExecutionFailed',
  COMMAND_TIMEOUT: 'commandTimeout',
  TOOL_EXECUTION_FAILED: 'toolExecutionFailed',
} as const;

export type LocalToolErrorCode =
  (typeof LOCAL_TOOL_ERROR_CODES)[keyof typeof LOCAL_TOOL_ERROR_CODES];

export enum LocalToolErrorCategory {
  FILE_SYSTEM = 'FILE_SYSTEM',
  VALIDATION = 'VALIDATION',
  SEARCH = 'SEARCH',
  PAGINATION = 'PAGINATION',
  EXECUTION = 'EXECUTION',
}

interface LocalToolErrorMetadata {
  code: LocalToolErrorCode;
  category: LocalToolErrorCategory;
  description: string;
  recoverability: 'recoverable' | 'unrecoverable' | 'user-action-required';
}

export const LOCAL_TOOL_ERROR_REGISTRY: Record<
  LocalToolErrorCode,
  LocalToolErrorMetadata
> = {
  [LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED]: {
    code: LOCAL_TOOL_ERROR_CODES.PATH_VALIDATION_FAILED,
    category: LocalToolErrorCategory.VALIDATION,
    description: 'Path validation failed - invalid or unsafe path',
    recoverability: 'user-action-required',
  },
  [LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED]: {
    code: LOCAL_TOOL_ERROR_CODES.FILE_ACCESS_FAILED,
    category: LocalToolErrorCategory.FILE_SYSTEM,
    description: 'Cannot access file - may not exist or lack permissions',
    recoverability: 'unrecoverable',
  },
  [LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED]: {
    code: LOCAL_TOOL_ERROR_CODES.FILE_READ_FAILED,
    category: LocalToolErrorCategory.FILE_SYSTEM,
    description: 'Failed to read file contents',
    recoverability: 'unrecoverable',
  },
  [LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE]: {
    code: LOCAL_TOOL_ERROR_CODES.FILE_TOO_LARGE,
    category: LocalToolErrorCategory.FILE_SYSTEM,
    description: 'File exceeds size limits for operation',
    recoverability: 'user-action-required',
  },
  [LOCAL_TOOL_ERROR_CODES.BINARY_FILE_UNSUPPORTED]: {
    code: LOCAL_TOOL_ERROR_CODES.BINARY_FILE_UNSUPPORTED,
    category: LocalToolErrorCategory.FILE_SYSTEM,
    description: 'Binary file is not supported by text content reader',
    recoverability: 'user-action-required',
  },

  [LOCAL_TOOL_ERROR_CODES.NO_MATCHES]: {
    code: LOCAL_TOOL_ERROR_CODES.NO_MATCHES,
    category: LocalToolErrorCategory.SEARCH,
    description: 'Search pattern found no matches',
    recoverability: 'user-action-required',
  },

  [LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE]: {
    code: LOCAL_TOOL_ERROR_CODES.OUTPUT_TOO_LARGE,
    category: LocalToolErrorCategory.PAGINATION,
    description: 'Output exceeds size limits',
    recoverability: 'user-action-required',
  },

  [LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE]: {
    code: LOCAL_TOOL_ERROR_CODES.COMMAND_NOT_AVAILABLE,
    category: LocalToolErrorCategory.EXECUTION,
    description: 'Required CLI command is not installed or not in PATH',
    recoverability: 'user-action-required',
  },
  [LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED]: {
    code: LOCAL_TOOL_ERROR_CODES.COMMAND_EXECUTION_FAILED,
    category: LocalToolErrorCategory.EXECUTION,
    description: 'System command execution failed',
    recoverability: 'unrecoverable',
  },
  [LOCAL_TOOL_ERROR_CODES.COMMAND_TIMEOUT]: {
    code: LOCAL_TOOL_ERROR_CODES.COMMAND_TIMEOUT,
    category: LocalToolErrorCategory.EXECUTION,
    description: 'Command execution timed out',
    recoverability: 'user-action-required',
  },
  [LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED]: {
    code: LOCAL_TOOL_ERROR_CODES.TOOL_EXECUTION_FAILED,
    category: LocalToolErrorCategory.EXECUTION,
    description: 'Generic tool execution failure',
    recoverability: 'unrecoverable',
  },
};
